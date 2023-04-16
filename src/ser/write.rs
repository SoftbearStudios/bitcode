use crate::nightly::div_ceil;

type Word = u64;
const WORD_BITS: usize = Word::BITS as usize;

pub trait Write {
    /// Writes up to 64 bits. The index of `word`'s most significant 1 must be < `bits`.
    /// `bits` must be in range `0..=64`.
    fn write_bits(&mut self, word: Word, bits: usize);
    /// Writes a bit.
    fn write_bit(&mut self, v: bool);
    /// Writes bytes. `bytes.len()`must be < `isize::MAX as usize / u8::BITS as usize`.
    fn write_bytes(&mut self, bytes: &[u8]);
}

pub trait WriteWith: Write + Default {
    fn clear(&mut self);
    fn into_inner(self) -> Vec<u8>;
}

#[cfg(target_endian = "little")]
pub type WriteWithImpl = SerVec;
#[cfg(target_endian = "big")]
pub type WriteWithImpl = BitVecImpl;

#[cfg(any(target_endian = "big", test))]
pub use bitvec_write::BitVecImpl;
#[cfg(any(target_endian = "big", test))]
mod bitvec_write {
    use super::*;
    use bitvec::prelude::*;

    pub type BitVecImpl = BitVec<u8, Lsb0>;

    impl Write for BitVecImpl {
        fn write_bits(&mut self, word: Word, bits: usize) {
            self.extend_from_bitslice(
                &BitSlice::<u8, Lsb0>::from_slice(&word.to_le_bytes())[..bits],
            );
        }

        fn write_bit(&mut self, v: bool) {
            self.push(v);
        }

        fn write_bytes(&mut self, bytes: &[u8]) {
            self.extend_from_bitslice(&BitSlice::<u8, Lsb0>::from_slice(bytes));
        }
    }

    impl WriteWith for BitVecImpl {
        fn clear(&mut self) {
            self.clear()
        }

        fn into_inner(mut self) -> Vec<u8> {
            self.force_align();
            self.into_vec()
        }
    }
}

#[derive(Debug, Default)]
pub struct SerVec {
    words: Box<[Word]>,
    len: usize,
}

impl SerVec {
    // Reserves up to an `index + 1` in words if a bounds check fails.
    // Returns a mutable array of [index, index + 1] to avoid bounds checks near hot code.
    #[cold]
    fn reserve_index_plus_one(&mut self, index: usize) -> &mut [Word; 2] {
        let index_plus_one = index + 1;

        let cap = index_plus_one + 1;
        let new_cap = cap.next_power_of_two().max(16);
        let new = bytemuck::allocation::zeroed_slice_box(new_cap);
        let previous = std::mem::replace(&mut self.words, new);
        self.words[..previous.len()].copy_from_slice(&previous);

        (&mut self.words[index..index + 2]).try_into().unwrap()
    }

    fn reserve(&mut self, bits: usize) {
        let len = self.len + bits + WORD_BITS + 1; // Add 1 since bits might be 0.
        let cap = self.words.len() * WORD_BITS;
        if len > cap {
            self.reserve_inner(len);
        }
    }

    #[cold]
    fn reserve_inner(&mut self, cap: usize) {
        let new_cap = div_ceil(cap, WORD_BITS).next_power_of_two().max(16);
        let new = bytemuck::allocation::zeroed_slice_box(new_cap);
        let previous = std::mem::replace(&mut self.words, new);
        self.words[..previous.len()].copy_from_slice(&previous);
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

        let bit_index = self.len;
        self.len += bits;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        // Only requires 1 branch in hot path.
        let slice = if let Some(w) = self.words.get_mut(index..index + 2) {
            w.try_into().unwrap()
        } else {
            out_of_bounds(self, index)
        };
        slice[0] |= word << bit_remainder;
        slice[1] |= word
            .checked_shr((WORD_BITS - bit_remainder) as u32)
            .unwrap_or(0);
    }

    fn write_reserved_bits(&mut self, word: Word, bits: usize) {
        self.write_bits_inner(word, bits, |_, _| unreachable!());
    }

    fn write_reserved_words(&mut self, src: &[Word]) {
        debug_assert!(!src.is_empty()); // TODO handle

        let bit_start = self.len;
        let bit_end = self.len + src.len() * WORD_BITS;
        self.len = bit_end;

        let start = bit_start / WORD_BITS;
        let end = div_ceil(bit_end, WORD_BITS);

        let shl = bit_start % WORD_BITS;
        let shr = WORD_BITS - shl;

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

impl Write for SerVec {
    fn write_bits(&mut self, word: Word, bits: usize) {
        self.write_bits_inner(word, bits, Self::reserve_index_plus_one);
    }

    fn write_bit(&mut self, v: bool) {
        let bit_index = self.len;
        self.len += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        *if let Some(w) = self.words.get_mut(index) {
            w
        } else {
            &mut self.reserve_index_plus_one(index)[0]
        } |= (v as Word) << bit_remainder;
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) {
        fn write_0_to_7_bytes(me: &mut SerVec, bytes: &[u8]) {
            debug_assert!(bytes.len() < 8);
            me.write_reserved_bits(
                read_0_to_7_bytes_into_word(bytes),
                bytes.len() * u8::BITS as usize,
            );
        }

        // Slower for small inputs.
        fn write_many_bytes(me: &mut SerVec, bytes: &[u8]) {
            // TODO look into align_to specification to see if any special cases are required.
            let (a, b, c) = bytemuck::pod_align_to::<u8, Word>(bytes);
            write_0_to_7_bytes(me, a);
            me.write_reserved_words(b);
            write_0_to_7_bytes(me, c);
        }

        self.reserve(bytes.len() * u8::BITS as usize);

        // Fast case for short bytes. Both methods are about the same speed at 75 bytes.
        if bytes.len() < 75 {
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

impl WriteWith for SerVec {
    fn clear(&mut self) {
        self.words[0..div_ceil(self.len, WORD_BITS)]
            .iter_mut()
            .for_each(|w| *w = 0);
        debug_assert!(self.words.iter().all(|&w| w == 0));
        self.len = 0;
    }

    fn into_inner(self) -> Vec<u8> {
        // Only copy words with their bits set.
        let words = &self.words[..div_ceil(self.len, Word::BITS as usize)];

        // Create new allocation since Vec<u64> can't be converted to Vec<u8>.
        let mut bytes: Vec<_> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        bytes.truncate(div_ceil(self.len, u8::BITS as usize));
        bytes
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;
    use test::{black_box, Bencher};

    // How many times each benchmark calls the function.
    const TIMES: usize = 1000;

    #[bench]
    fn bench_vec(b: &mut Bencher) {
        let mut t = vec![];
        b.iter(|| {
            // Don't measure growth speed.
            let t = black_box(&mut t);
            t.clear();

            for _ in 0..TIMES {
                t.push(black_box(0b10101u8))
            }
            black_box(t);
        });
    }

    fn bench_write_bit<T: WriteWith>(b: &mut Bencher) {
        let mut t = T::default();
        b.iter(|| {
            // Don't measure growth speed.
            let t = black_box(&mut t);
            t.clear();

            for _ in 0..TIMES {
                t.write_bit(black_box(true))
            }
            black_box(t);
        });
    }

    fn bench_write_bytes<T: WriteWith>(b: &mut Bencher, bytes: usize) {
        let v = vec![123u8; bytes];
        let mut t = T::default();

        b.iter(|| {
            // Don't measure growth speed.
            let t = black_box(&mut t);
            t.clear();

            t.write_bit(true); // Make it unaligned.
            for _ in 0..TIMES {
                t.write_bytes(black_box(v.as_slice()))
            }

            black_box(t);
        });
    }

    fn bench_write_bits<T: WriteWith>(b: &mut Bencher, bits: usize) {
        let v = Word::MAX >> (Word::BITS as usize - bits);
        let mut t = T::default();

        b.iter(|| {
            // Don't measure growth speed.
            let t = black_box(&mut t);
            t.clear();

            for _ in 0..TIMES {
                t.write_bits(black_box(v), black_box(bits))
            }
            black_box(t);
        });
    }

    #[bench]
    fn bench_bit_vec_write_bit1(b: &mut Bencher) {
        bench_write_bit::<BitVecImpl>(b);
    }

    #[bench]
    fn bench_ser_vec_write_bit1(b: &mut Bencher) {
        bench_write_bit::<SerVec>(b);
    }

    #[bench]
    fn bench_bit_vec_write_bits_5(b: &mut Bencher) {
        bench_write_bits::<BitVecImpl>(b, 5);
    }

    #[bench]
    fn bench_ser_vec_write_bits_5(b: &mut Bencher) {
        bench_write_bits::<SerVec>(b, 5);
    }

    #[bench]
    fn bench_bit_vec_write_bits_41(b: &mut Bencher) {
        bench_write_bits::<BitVecImpl>(b, 41);
    }

    #[bench]
    fn bench_ser_vec_write_bits_41(b: &mut Bencher) {
        bench_write_bits::<SerVec>(b, 41);
    }

    #[bench]
    fn bench_bit_vec_write_bytes_5(b: &mut Bencher) {
        bench_write_bytes::<BitVecImpl>(b, 5);
    }

    #[bench]
    fn bench_ser_vec_write_bytes_5(b: &mut Bencher) {
        bench_write_bytes::<SerVec>(b, 5);
    }

    #[bench]
    fn bench_bit_vec_write_bytes_100(b: &mut Bencher) {
        bench_write_bytes::<BitVecImpl>(b, 100);
    }

    #[bench]
    fn bench_ser_vec_write_bytes_10(b: &mut Bencher) {
        bench_write_bytes::<SerVec>(b, 10);
    }

    #[bench]
    fn bench_ser_vec_write_bytes_20(b: &mut Bencher) {
        bench_write_bytes::<SerVec>(b, 20);
    }

    #[bench]
    fn bench_ser_vec_write_bytes_100(b: &mut Bencher) {
        bench_write_bytes::<SerVec>(b, 100);
    }

    #[bench]
    fn bench_ser_vec_write_bytes_1000(b: &mut Bencher) {
        bench_write_bytes::<SerVec>(b, 1000);
    }
}
