use crate::buffer::BufferTrait;
use crate::encoding::ByteEncoding;
use crate::nightly::div_ceil;
use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::{Result, E};
use from_bytes_or_zeroed::FromBytesOrZeroed;
use std::array;

/// A fast [`Buffer`] that operates on [`Word`]s.
#[derive(Debug, Default)]
pub struct WordBuffer {
    allocation: Allocation,
    read_bytes_buf: Box<[Word]>,
}

#[derive(Debug, Default)]
struct Allocation {
    allocation: Vec<Word>,
    written_words: usize,
}

impl Allocation {
    fn as_mut_slice(&mut self) -> &mut [Word] {
        self.allocation.as_mut_slice()
    }

    fn take_box(&mut self) -> Box<[Word]> {
        let vec = std::mem::take(&mut self.allocation);
        let mut box_ = if vec.capacity() == vec.len() {
            vec
        } else {
            // Must have been created by start_read. We need len and capacity to be equal to make
            // into_boxed_slice zero cost. If we zeroed up to capacity we could have a situation
            // where reading/writing to same buffer causes the whole capacity to be zeroed each
            // write (even if only a small portion of the buffer is used).
            vec![]
        }
        .into_boxed_slice();

        // Zero all the words that we could have written to.
        let written_words = self.written_words.min(box_.len());
        box_[0..written_words].fill(0);
        self.written_words = 0;
        debug_assert!(box_.iter().all(|&w| w == 0));

        box_
    }

    fn replace_box(&mut self, box_: Box<[Word]>, written_words: usize) {
        self.allocation = box_.into();
        self.written_words = written_words;
    }

    fn make_vec(&mut self) -> &mut Vec<Word> {
        self.written_words = usize::MAX;
        &mut self.allocation
    }
}

pub struct WordContext {
    input_bytes: usize,
}

impl WordBuffer {
    /// Extra [`Word`]s appended to the end of the input to make deserialization faster.
    /// 1 for peek_reserved_bits and another for read_zeros (which calls peek_reserved_bits).
    const READ_PADDING: usize = 2;
}

impl BufferTrait for WordBuffer {
    type Writer = WordWriter;
    type Reader<'a> = WordReader<'a>;
    type Context = WordContext;

    fn capacity(&self) -> usize {
        // Subtract the padding of 1 (added by alloc_index_plus_one).
        self.allocation.allocation.capacity().saturating_sub(1) * WORD_BYTES
    }

    fn with_capacity(cap: usize) -> Self {
        let mut me = Self::default();
        if cap == 0 {
            return me;
        }
        let mut writer = Self::Writer::default();

        // Convert len to index by subtracting 1.
        Self::Writer::alloc_index_plus_one(&mut writer.words, div_ceil(cap, WORD_BYTES) - 1);
        me.allocation.replace_box(writer.words, 0);
        me
    }

    fn start_write(&mut self) -> Self::Writer {
        let words = self.allocation.take_box();
        Self::Writer { words, index: 0 }
    }

    fn finish_write(&mut self, mut writer: Self::Writer) -> &[u8] {
        // write_zeros doesn't allocate, but it moves index so we allocate up to index at the end.
        let index = writer.index / WORD_BITS;
        if index >= writer.words.len() {
            // TODO could allocate exact amount instead of regular growth strategy.
            Self::Writer::alloc_index_plus_one(&mut writer.words, index);
        }

        let Self::Writer { words, index } = writer;
        let written_words = div_ceil(index, WORD_BITS);

        self.allocation.replace_box(words, written_words);
        let written_words = &mut self.allocation.as_mut_slice()[..written_words];

        // Swap bytes in each word (that was written to) if big endian.
        if cfg!(target_endian = "big") {
            written_words.iter_mut().for_each(|w| *w = w.swap_bytes());
        }

        let written_bytes = div_ceil(index, u8::BITS as usize);
        &bytemuck::cast_slice(written_words)[..written_bytes]
    }

    fn start_read<'a>(&'a mut self, bytes: &'a [u8]) -> (Self::Reader<'a>, Self::Context) {
        let words = self.allocation.make_vec();
        words.clear();

        // u8s rounded up to u64s plus 1 u64 padding.
        let capacity = div_ceil(bytes.len(), WORD_BYTES) + Self::READ_PADDING;
        words.reserve_exact(capacity);

        // Fast hot loop (would be nicer with array_chunks, but that requires nightly).
        let chunks = bytes.chunks_exact(WORD_BYTES);
        let remainder = chunks.remainder();
        words.extend(chunks.map(|chunk| {
            let chunk: &[u8; 8] = chunk.try_into().unwrap();
            Word::from_le_bytes(*chunk)
        }));

        // Remaining bytes.
        if !remainder.is_empty() {
            words.push(u64::from_le_bytes(array::from_fn(|i| {
                remainder.get(i).copied().unwrap_or_default()
            })));
        }

        // Padding so peek_reserved_bits doesn't ever go out of bounds.
        words.extend([0; Self::READ_PADDING]);
        debug_assert_eq!(words.len(), capacity);

        let reader = WordReader {
            inner: WordReaderInner { words, index: 0 },
            read_bytes_buf: &mut self.read_bytes_buf,
        };
        let context = WordContext {
            input_bytes: bytes.len(),
        };
        (reader, context)
    }

    fn finish_read(reader: Self::Reader<'_>, context: Self::Context) -> Result<()> {
        let read = reader.inner.index;
        let bytes_read = div_ceil(read, u8::BITS as usize);
        let index = read / WORD_BITS;
        let bits_written = read % WORD_BITS;

        if bits_written != 0 && reader.inner.words[index] & !((1 << bits_written) - 1) != 0 {
            return Err(E::ExpectedEof.e());
        }

        use std::cmp::Ordering::*;
        match bytes_read.cmp(&context.input_bytes) {
            Less => Err(E::ExpectedEof.e()),
            Equal => Ok(()),
            Greater => {
                // It is possible that we read more bytes than we have (bytes are rounded up to words).
                // We don't check this while deserializing to avoid degrading performance.
                Err(E::Eof.e())
            }
        }
    }
}

#[derive(Default)]
pub struct WordWriter {
    words: Box<[Word]>,
    index: usize,
}

impl WordWriter {
    /// Allocates at least `words` of zeroed memory.
    fn alloc(words: &mut Box<[Word]>, len: usize) {
        let new_cap = len.next_power_of_two().max(16);

        // TODO find a way to use Allocator::grow_zeroed safely (new bytemuck api?).
        let new = bytemuck::allocation::zeroed_slice_box(new_cap);

        let previous = std::mem::replace(words, new);
        words[..previous.len()].copy_from_slice(&previous);
    }

    // Allocates up to an `index + 1` in words if a bounds check fails.
    // Returns a mutable array of [index, index + 1] to avoid bounds checks near hot code.
    #[cold]
    fn alloc_index_plus_one(words: &mut Box<[Word]>, index: usize) -> &mut [Word; 2] {
        let end = index + 2;
        Self::alloc(words, end);
        (&mut words[index..end]).try_into().unwrap()
    }

    /// Ensures that space for `bytes` is allocated.\
    #[inline(always)]
    fn reserve_write_bytes(&mut self, bytes: usize) {
        let index = self.index / WORD_BITS + bytes / WORD_BYTES + 1;
        if index >= self.words.len() {
            Self::alloc_index_plus_one(&mut self.words, index);
        }
    }

    #[inline(always)]
    fn write_bits_inner(
        &mut self,
        word: Word,
        bits: usize,
        out_of_bounds: fn(&mut Box<[Word]>, usize) -> &mut [Word; 2],
    ) {
        debug_assert!(bits <= WORD_BITS);
        if bits != WORD_BITS {
            debug_assert_eq!(word, word & ((1 << bits) - 1));
        }

        let bit_index = self.index;
        self.index += bits;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        // Only requires 1 branch in hot path.
        let slice = if let Some(w) = self.words.get_mut(index..index + 2) {
            w.try_into().unwrap()
        } else {
            out_of_bounds(&mut self.words, index)
        };
        slice[0] |= word << bit_remainder;
        slice[1] = (word >> 1) >> (WORD_BITS - bit_remainder - 1);
    }

    #[inline(always)]
    fn write_reserved_bits(&mut self, word: Word, bits: usize) {
        self.write_bits_inner(word, bits, |_, _| unreachable!());
    }

    fn write_reserved_words(&mut self, src: &[Word]) {
        debug_assert!(!src.is_empty());

        let bit_start = self.index;
        let bit_end = self.index + src.len() * WORD_BITS;
        self.index = bit_end;

        let start = bit_start / WORD_BITS;
        let end = div_ceil(bit_end, WORD_BITS);

        let shl = bit_start % WORD_BITS;
        let shr = WORD_BITS - shl;

        // TODO use word_copy.
        if shl == 0 {
            self.words[start..end].copy_from_slice(src)
        } else {
            let after_start = start + 1;
            let before_end = end - 1;

            let dst = &mut self.words[after_start..before_end];

            // Do bounds check outside loop. Makes compiler go brrr
            assert!(dst.len() < src.len());

            for (i, w) in dst.iter_mut().enumerate() {
                let a = src[i];
                let b = src[i + 1];
                debug_assert_eq!(*w, 0);
                *w = (a >> shr) | (b << shl)
            }

            self.words[start] |= src[0] << shl;
            debug_assert_eq!(self.words[before_end], 0);
            self.words[before_end] = *src.last().unwrap() >> shr
        }
    }
}

impl Write for WordWriter {
    type Revert = usize;
    #[inline(always)]
    fn get_revert(&mut self) -> Self::Revert {
        self.index
    }
    #[cold]
    fn revert(&mut self, revert: Self::Revert) {
        // min with self.words.len() since if writing zeros, the words might not have been allocated.
        let start = div_ceil(revert, WORD_BITS).min(self.words.len());
        let end = (div_ceil(self.index, WORD_BITS)).min(self.words.len());

        // Zero whole words.
        self.words[start..end].fill(0);

        // Zero remaining bits. Might not have been allocated if writing zeros.
        let i = revert / WORD_BITS;
        if i < self.words.len() {
            let keep_up_to = revert % WORD_BITS;
            self.words[i] &= (1 << keep_up_to) - 1;
        }
        self.index = revert;
    }

    #[inline(always)]
    fn write_bit(&mut self, v: bool) {
        let bit_index = self.index;
        self.index += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        *if let Some(w) = self.words.get_mut(index) {
            w
        } else {
            &mut Self::alloc_index_plus_one(&mut self.words, index)[0]
        } |= (v as Word) << bit_remainder;
    }

    #[inline(always)]
    fn write_bits(&mut self, word: Word, bits: usize) {
        self.write_bits_inner(word, bits, Self::alloc_index_plus_one);
    }

    #[inline(always)]
    fn write_bytes(&mut self, bytes: &[u8]) {
        #[inline(always)]
        fn write_0_to_8_bytes(me: &mut WordWriter, bytes: &[u8]) {
            debug_assert!(bytes.len() <= 8);
            me.write_reserved_bits(
                u64::from_le_bytes_or_zeroed(bytes),
                bytes.len() * u8::BITS as usize,
            );
        }

        // Slower for small inputs. Doesn't work on big endian since it bytemucks u64 to bytes.
        #[inline(never)]
        fn write_many_bytes(me: &mut WordWriter, bytes: &[u8]) {
            assert!(!cfg!(target_endian = "big"));

            // TODO look into align_to specification to see if any special cases are required.
            let (a, b, c) = bytemuck::pod_align_to::<u8, Word>(bytes);
            write_0_to_8_bytes(me, a);
            me.write_reserved_words(b);
            write_0_to_8_bytes(me, c);
        }

        if bytes.is_empty() {
            return;
        }

        self.reserve_write_bytes(bytes.len());

        // Fast case for short bytes. Both methods are about the same speed at 75 bytes.
        // write_many_bytes doesn't work on big endian.
        if bytes.len() < 75 || cfg!(target_endian = "big") {
            let mut bytes = bytes;
            while bytes.len() > 8 {
                let b8: &[u8; 8] = bytes[0..8].try_into().unwrap();
                self.write_reserved_bits(Word::from_le_bytes(*b8), WORD_BITS);
                bytes = &bytes[8..]
            }
            write_0_to_8_bytes(self, bytes);
        } else {
            write_many_bytes(self, bytes)
        }
    }

    #[inline(always)]
    fn write_encoded_bytes<C: ByteEncoding>(&mut self, mut bytes: &[u8]) -> bool {
        // TODO could reserve bytes.len() * C::BITS_PER_BYTE.

        while bytes.len() > 8 {
            let (bytes8, remaining) = bytes.split_at(8);
            let bytes8: &[u8; 8] = bytes8.try_into().unwrap();
            bytes = remaining;

            let word = Word::from_le_bytes(*bytes8);
            if !C::validate(word, WORD_BYTES) {
                return false;
            }
            self.write_bits(C::pack(word), WORD_BYTES * C::BITS_PER_BYTE);
        }

        let word = Word::from_le_bytes_or_zeroed(bytes);
        if !C::validate(word, bytes.len()) {
            return false;
        }
        self.write_bits(C::pack(word), bytes.len() * C::BITS_PER_BYTE);
        true
    }

    #[inline(always)]
    fn write_zeros(&mut self, bits: usize) {
        debug_assert!(bits <= WORD_BITS);
        self.index += bits;
    }
}

struct WordReaderInner<'a> {
    words: &'a [Word],
    index: usize,
}

impl WordReaderInner<'_> {
    #[inline(always)]
    fn peek_reserved_bits(&self, bits: usize) -> Word {
        debug_assert!((1..=WORD_BITS).contains(&bits));
        let bit_index = self.index;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        let a = self.words[index] >> bit_remainder;
        let b = (self.words[index + 1] << 1) << (WORD_BITS - 1 - bit_remainder);

        // Clear bits at end (don't need to do in ser because bits at end are zeroed).
        let extra_bits = WORD_BITS - bits;
        ((a | b) << extra_bits) >> extra_bits
    }

    #[inline(always)]
    fn read_reserved_bits(&mut self, bits: usize) -> Word {
        let v = self.peek_reserved_bits(bits);
        self.index += bits;
        v
    }

    #[inline(never)]
    fn read_reserved_words(&mut self, dst: &mut [Word]) {
        let start = self.index / WORD_BITS;
        let offset = self.index % WORD_BITS;
        self.index += dst.len() * WORD_BITS;

        let end = start + div_ceil(offset, WORD_BITS) + dst.len();
        let src = &self.words[start..end];
        word_copy(src, dst, offset);
    }

    /// Faster [`Self::reserve_read_bytes`] that can elide bounds checks for `bits` in 1..=64.
    #[inline(always)]
    fn reserve_read_1_to_64(&self, bits: usize) -> Result<()> {
        debug_assert!((1..=WORD_BITS).contains(&bits));

        let read = self.index / WORD_BITS;
        let len = self.words.len();
        if read + 1 >= len {
            // TODO hint as unlikely.
            Err(E::Eof.e())
        } else {
            Ok(())
        }
    }

    /// Checks that `bytes` exist.
    #[inline(always)]
    fn reserve_read_bytes(&self, bytes: usize) -> Result<()> {
        let whole_words_len = bytes / WORD_BYTES;

        let read = self.index / WORD_BITS + 1 + whole_words_len;
        if read >= self.words.len() {
            // TODO hint as unlikely.
            Err(E::Eof.e())
        } else {
            Ok(())
        }
    }
}

pub struct WordReader<'a> {
    inner: WordReaderInner<'a>,
    read_bytes_buf: &'a mut Box<[Word]>,
}

impl<'a> Read for WordReader<'a> {
    #[inline(always)]
    fn advance(&mut self, bits: usize) {
        self.inner.index += bits;
    }

    #[inline(always)]
    fn peek_bits(&mut self) -> Result<Word> {
        self.inner.reserve_read_1_to_64(64)?;
        Ok(self.inner.peek_reserved_bits(64))
    }

    #[inline(always)]
    fn read_bit(&mut self) -> Result<bool> {
        self.inner.reserve_read_1_to_64(1)?;

        let bit_index = self.inner.index;
        self.inner.index += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        Ok((self.inner.words[index] & (1 << bit_remainder)) != 0)
    }

    #[inline(always)]
    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        self.inner.reserve_read_1_to_64(bits)?;
        Ok(self.inner.read_reserved_bits(bits))
    }

    #[inline(always)]
    fn read_bytes(&mut self, len: usize) -> Result<&[u8]> {
        // TODO get this to elide bounds checks.
        self.inner.reserve_read_bytes(len)?;

        // Only allocate after reserve_read to prevent memory exhaustion attacks.
        let whole_words_len = len / WORD_BYTES;
        let word_len = whole_words_len + 1;

        let buf = &mut *self.read_bytes_buf;
        let words = if let Some(slice) = buf.get_mut(..word_len) {
            slice
        } else {
            alloc_read_bytes_buf(buf, word_len);
            &mut buf[..word_len]
        };

        let whole_words = &mut words[..whole_words_len];
        if whole_words.len() < 4 {
            for w in whole_words {
                *w = self.inner.read_reserved_bits(WORD_BITS);
            }
        } else {
            self.inner.read_reserved_words(whole_words);
        }

        // We can read the whole word (the caller will ignore the extra).
        // We even read it if we'll use none of it's bytes to avoid a branch.
        *words.last_mut().unwrap() = self.inner.peek_reserved_bits(WORD_BITS);
        self.inner.index += (len % WORD_BYTES) * u8::BITS as usize;

        // Swap bytes in each word (that was written to) if big endian and bytemuck to bytes.
        if cfg!(target_endian = "big") {
            words.iter_mut().for_each(|w| *w = w.swap_bytes());
        }
        Ok(&bytemuck::cast_slice(self.read_bytes_buf)[..len])
    }

    #[inline(always)]
    fn read_encoded_bytes<C: ByteEncoding>(&mut self, len: usize) -> Result<&[u8]> {
        // Early return on len 0 so we can compute len - 1.
        if len == 0 {
            return Ok(&[]);
        }

        let whole_words_len = (len - 1) / WORD_BYTES;
        let word_len = whole_words_len + 1;

        // Only allocate after reserved to prevent memory exhaustion attacks.
        let read = self.inner.index / WORD_BITS + 2 + whole_words_len * C::BITS_PER_BYTE / 8;
        if read >= self.inner.words.len() {
            return Err(E::Eof.e());
        }

        let buf = &mut *self.read_bytes_buf;
        let words = if let Some(slice) = buf.get_mut(..word_len) {
            slice
        } else {
            alloc_read_bytes_buf(buf, word_len);
            &mut buf[..word_len]
        };

        let whole_words = &mut words[..whole_words_len];
        for w in whole_words {
            *w = C::unpack(self.inner.peek_reserved_bits(WORD_BITS));
            self.inner.index += WORD_BYTES * C::BITS_PER_BYTE;
        }

        let remaining_bytes = len - whole_words_len * WORD_BYTES;
        debug_assert!((1..=8).contains(&remaining_bytes));
        *words.last_mut().unwrap() = C::unpack(self.inner.peek_reserved_bits(WORD_BITS));
        self.inner.index += remaining_bytes * C::BITS_PER_BYTE as usize;

        // Swap bytes in each word (that was written to) if big endian and bytemuck to bytes.
        if cfg!(target_endian = "big") {
            words.iter_mut().for_each(|w| *w = w.swap_bytes());
        }
        Ok(&bytemuck::cast_slice(self.read_bytes_buf)[..len])
    }

    #[inline(always)]
    fn reserve_bits(&self, bits: usize) -> Result<()> {
        self.inner.reserve_read_bytes(bits / u8::BITS as usize)
    }
}

#[cold]
fn alloc_read_bytes_buf(buf: &mut Box<[Word]>, len: usize) {
    let new_cap = len.next_power_of_two().max(16);
    *buf = bytemuck::allocation::zeroed_slice_box(new_cap);
}

/// Copies words from `src` to `dst` offset by `offset`.
fn word_copy(src: &[Word], dst: &mut [Word], offset: usize) {
    if offset == 0 {
        dst.copy_from_slice(src)
    } else {
        debug_assert_eq!(src.len(), dst.len() + 1);

        let shl = WORD_BITS - offset;
        let shr = offset;

        // Do bounds check outside loop. Makes compiler go brrr
        assert!(dst.len() < src.len());

        for (i, w) in dst.iter_mut().enumerate() {
            let a = src[i];
            let b = src[i + 1];
            *w = (a >> shr) | (b << shl)
        }
    }
}
