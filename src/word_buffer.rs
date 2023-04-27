use crate::buffer::WithCapacity;
use crate::nightly::div_ceil;
use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::{Result, E};
use std::array;

/// A fast [`Buffer`] that operates on [`Word`]s.
#[derive(Debug, Default)]
pub struct WordBuffer {
    words: Vec<Word>,
    index: usize,
    input_bytes: usize,
    read_bytes_buf: Box<[Word]>,
}

impl WithCapacity for WordBuffer {
    fn capacity(&self) -> usize {
        // Subtract the padding of 1 (added by alloc_index_plus_one).
        self.words.len().saturating_sub(1) * WORD_BYTES
    }

    fn with_capacity(cap: usize) -> Self {
        let mut me = Self::default();
        if cap == 0 {
            return me;
        }

        // Convert len to index by subtracting 1.
        me.alloc_index_plus_one(div_ceil(cap, WORD_BYTES) - 1);
        me
    }
}

impl WordBuffer {
    /// Allocates at least `words` of zeroed memory.
    /// TODO find a way to use Allocator::grow_zeroed safely (new bytemuck api?).
    fn alloc(&mut self, words: usize) {
        let new_cap = words.next_power_of_two().max(16);
        let new = bytemuck::allocation::zeroed_slice_box(new_cap);
        let previous = std::mem::replace(&mut self.words, Vec::from(new));
        self.words[..previous.len()].copy_from_slice(&previous);
    }

    // Allocates up to an `index + 1` in words if a bounds check fails.
    // Returns a mutable array of [index, index + 1] to avoid bounds checks near hot code.
    #[cold]
    fn alloc_index_plus_one(&mut self, index: usize) -> &mut [Word; 2] {
        let end = index + 2;
        self.alloc(end);
        (&mut self.words[index..end]).try_into().unwrap()
    }

    /// Ensures that space for `bytes` is allocated.
    fn reserve_write_bytes(&mut self, bytes: usize) {
        let index = self.index / WORD_BITS + bytes / WORD_BYTES + 1;
        if index >= self.words.len() {
            self.alloc_index_plus_one(index);
        }
    }

    fn write_bits_inner(
        &mut self,
        word: Word,
        bits: usize,
        out_of_bounds: fn(&mut Self, usize) -> &mut [Word; 2],
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
            out_of_bounds(self, index)
        };
        slice[0] |= word << bit_remainder;
        slice[1] = (word >> 1) >> (WORD_BITS - bit_remainder - 1);
    }

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

impl Write for WordBuffer {
    fn start_write(&mut self) {
        let max_index =
            (div_ceil(self.index, WORD_BITS)).max(div_ceil(self.input_bytes, WORD_BYTES));
        self.index = 0;
        self.input_bytes = 0;

        // Zero all the words that we could have written to.
        self.words[0..max_index].fill(0);
        debug_assert!(self.words.iter().all(|&w| w == 0));
    }

    fn finish_write(&mut self) -> &[u8] {
        let written_words = &mut self.words[..div_ceil(self.index, WORD_BITS)];

        // Swap bytes in each word (that was written to) if big endian.
        if cfg!(target_endian = "big") {
            written_words.iter_mut().for_each(|w| *w = w.swap_bytes());
        }
        &bytemuck::cast_slice(written_words)[..div_ceil(self.index, u8::BITS as usize)]
    }

    fn write_bits(&mut self, word: Word, bits: usize) {
        self.write_bits_inner(word, bits, Self::alloc_index_plus_one);
    }

    fn write_bit(&mut self, v: bool) {
        let bit_index = self.index;
        self.index += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        *if let Some(w) = self.words.get_mut(index) {
            w
        } else {
            &mut self.alloc_index_plus_one(index)[0]
        } |= (v as Word) << bit_remainder;
    }

    #[inline(always)] // Improves perf (regular #[inline] isn't enough).
    fn write_bytes(&mut self, bytes: &[u8]) {
        #[inline]
        fn write_0_to_7_bytes(me: &mut WordBuffer, bytes: &[u8]) {
            debug_assert!(bytes.len() < 8);
            me.write_reserved_bits(
                read_0_to_7_bytes_into_word(bytes),
                bytes.len() * u8::BITS as usize,
            );
        }

        // Slower for small inputs. Doesn't work on big endian since it bytemucks u64 to bytes.
        #[inline(never)]
        fn write_many_bytes(me: &mut WordBuffer, bytes: &[u8]) {
            assert!(!cfg!(target_endian = "big"));

            // TODO look into align_to specification to see if any special cases are required.
            let (a, b, c) = bytemuck::pod_align_to::<u8, Word>(bytes);
            write_0_to_7_bytes(me, a);
            me.write_reserved_words(b);
            write_0_to_7_bytes(me, c);
        }

        self.reserve_write_bytes(bytes.len());

        // Fast case for short bytes. Both methods are about the same speed at 75 bytes.
        // write_many_bytes doesn't work on big endian.
        if bytes.len() < 75 || cfg!(target_endian = "big") {
            let mut bytes = bytes;
            while bytes.len() >= 8 {
                let b8: &[u8; 8] = bytes[0..8].try_into().unwrap();
                self.write_reserved_bits(Word::from_le_bytes(*b8), WORD_BITS);
                bytes = &bytes[8..]
            }
            write_0_to_7_bytes(self, bytes);
        } else {
            write_many_bytes(self, bytes)
        }
    }
}

#[inline]
fn read_0_to_7_bytes_into_word(mut bytes: &[u8]) -> Word {
    // Faster than Word -> &mut [u8; 8] + copy_from_slice.
    let mut ret = 0;
    let mut shift = 0;
    if let Some(b4) = bytes.get(0..4) {
        bytes = &bytes[4..];
        let b4: &[u8; 4] = b4.try_into().unwrap();
        ret |= u32::from_le_bytes(*b4) as Word;
        shift += u32::BITS;
    }
    if let Some(b2) = bytes.get(0..2) {
        bytes = &bytes[2..];
        let b2: &[u8; 2] = b2.try_into().unwrap();
        ret |= (u16::from_le_bytes(*b2) as Word) << shift;
        shift += u16::BITS;
    }
    if let Some(&b) = bytes.first() {
        ret |= (b as Word) << shift;
    }
    ret
}

impl WordBuffer {
    /// Extra [`Word`]s appended to the end of the input to make deserialization faster.
    /// 1 for peek_reserved_bits and another for read_zeros (which calls peek_reserved_bits).
    const READ_PADDING: usize = 2;

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

impl Read for WordBuffer {
    fn start_read(&mut self, bytes: &[u8]) {
        self.words.clear();
        self.index = 0;
        self.input_bytes = bytes.len();

        // u8s rounded up to u64s plus 1 u64 padding.
        let capacity = div_ceil(bytes.len(), WORD_BYTES) + Self::READ_PADDING;
        self.words.reserve_exact(capacity);

        // Fast hot loop (would be nicer with array_chunks, but that requires nightly).
        let chunks = bytes.chunks_exact(WORD_BYTES);
        let remainder = chunks.remainder();
        self.words.extend(chunks.map(|chunk| {
            let chunk: &[u8; 8] = chunk.try_into().unwrap();
            Word::from_le_bytes(*chunk)
        }));

        // Remaining bytes.
        if !remainder.is_empty() {
            self.words.push(u64::from_le_bytes(array::from_fn(|i| {
                remainder.get(i).copied().unwrap_or_default()
            })));
        }

        // Padding so peek_reserved_bits doesn't ever go out of bounds.
        self.words.extend([0; Self::READ_PADDING]);
        debug_assert_eq!(self.words.len(), capacity);
    }

    fn finish_read(&self) -> Result<()> {
        let read = self.index;
        let bytes_read = div_ceil(read, u8::BITS as usize);
        let index = read / WORD_BITS;
        let bits_written = read % WORD_BITS;

        if bits_written != 0 && self.words[index] & !((1 << bits_written) - 1) != 0 {
            return Err(E::ExpectedEof.e());
        }

        use std::cmp::Ordering::*;
        match bytes_read.cmp(&self.input_bytes) {
            Less => Err(E::ExpectedEof.e()),
            Equal => Ok(()),
            Greater => {
                // It is possible that we read more bytes than we have (bytes are rounded up to words).
                // We don't check this while deserializing to avoid degrading performance.
                Err(E::Eof.e())
            }
        }
    }

    fn advance(&mut self, bits: usize) -> Result<()> {
        self.index += bits;
        Ok(())
    }

    fn peek_bits(&mut self) -> Result<Word> {
        self.reserve_read_1_to_64(64)?;
        Ok(self.peek_reserved_bits(64))
    }

    fn read_bit(&mut self) -> Result<bool> {
        self.reserve_read_1_to_64(1)?;

        let bit_index = self.index;
        self.index += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        Ok((self.words[index] & (1 << bit_remainder)) != 0)
    }

    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        self.reserve_read_1_to_64(bits)?;
        Ok(self.read_reserved_bits(bits))
    }

    #[inline]
    fn read_bytes(&mut self, len: usize) -> Result<&[u8]> {
        // TODO get this to elide bounds checks.
        self.reserve_read_bytes(len)?;

        // Only allocate after reserve_read to prevent memory exhaustion attacks.
        let whole_words_len = len / WORD_BYTES;
        let word_len = whole_words_len + 1;

        // Take to avoid borrowing issue.
        let mut buf = std::mem::take(&mut self.read_bytes_buf);
        let words = if let Some(slice) = buf.get_mut(..word_len) {
            slice
        } else {
            #[cold]
            fn alloc_buf(buf: &mut Box<[Word]>, len: usize) {
                let new_cap = len.next_power_of_two().max(16);
                *buf = bytemuck::allocation::zeroed_slice_box(new_cap);
            }
            alloc_buf(&mut buf, word_len);
            &mut buf[..word_len]
        };

        let whole_words = &mut words[..whole_words_len];
        if whole_words.len() < 4 {
            for w in whole_words {
                *w = self.read_reserved_bits(WORD_BITS);
            }
        } else {
            self.read_reserved_words(whole_words);
        }

        // We can read the whole word (the caller will ignore the extra).
        // We even read it if we'll use none of it's bytes to avoid a branch.
        *words.last_mut().unwrap() = self.peek_reserved_bits(WORD_BITS);
        self.index += (len % WORD_BYTES) * u8::BITS as usize;

        // Swap bytes in each word (that was written to) if big endian.
        if cfg!(target_endian = "big") {
            words.iter_mut().for_each(|w| *w = w.swap_bytes());
        }

        // Replace and reborrow to avoid borrowing issue.
        self.read_bytes_buf = buf;
        Ok(&bytemuck::cast_slice(&self.read_bytes_buf)[..len])
    }

    fn reserve_bits(&self, bits: usize) -> Result<()> {
        self.reserve_read_bytes(bits / u8::BITS as usize)
    }
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
