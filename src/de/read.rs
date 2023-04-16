use crate::nightly::div_ceil;
use crate::{Result, E};
use std::array;
use std::marker::PhantomData;

type Word = u64;
const WORD_BITS: usize = Word::BITS as usize;
const WORD_BYTES: usize = std::mem::size_of::<Word>();

pub trait Read {
    fn finish(self) -> Result<()>;
    /// Reads up to 64 bits. `bits` must be in range `1..=64`.
    fn read_bits(&mut self, bits: usize) -> Result<Word>;
    // Reads 1 bit.
    fn read_bit(&mut self) -> Result<bool>;
    /// Reads `len` bytes. `len` must be < `isize::MAX as usize / u8::BITS as usize`.
    fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>>;
    /// Reads as many zeros as possible up to `max`. `max` must be in range `1..=63`.
    fn read_zeros(&mut self, max: usize) -> Result<usize>;
}

pub trait ReadWith<'a>: Read {
    fn from_inner(inner: &'a [u8]) -> Self;
}

#[cfg(target_endian = "little")]
pub type ReadWithImpl<'a> = DeVec<'a>;
#[cfg(target_endian = "big")]
pub type ReadWithImpl<'a> = BitSliceImpl<'a>;

#[cfg(any(target_endian = "big", test))]
pub use bitvec_read::BitSliceImpl;
#[cfg(any(target_endian = "big", test))]
mod bitvec_read {
    use crate::E;

    use super::*;
    use bitvec::domain::Domain;
    use bitvec::prelude::*;

    pub type BitSliceImpl<'a> = &'a BitSlice<u8, Lsb0>;

    impl<'a> Read for BitSliceImpl<'a> {
        fn finish(self) -> Result<()> {
            if self.is_empty() {
                return Ok(());
            }

            let e = match self.domain() {
                Domain::Enclave(e) => e,
                Domain::Region { head, body, tail } => {
                    if !body.is_empty() {
                        return Err(E::ExpectedEOF.e());
                    }
                    head.xor(tail).ok_or(E::ExpectedEOF.e())?
                }
            };
            (e.into_bitslice().count_ones() == 0)
                .then_some(())
                .ok_or(E::ExpectedEOF.e())
        }

        fn read_bits(&mut self, bits: usize) -> Result<Word> {
            let slice = self.get(..bits).ok_or(E::EOF.e())?;
            *self = &self[bits..];

            let mut v = [0; 8];
            BitSlice::<u8, Lsb0>::from_slice_mut(&mut v)[..bits].copy_from_bitslice(slice);
            Ok(Word::from_le_bytes(v))
        }

        fn read_bit(&mut self) -> Result<bool> {
            let v = *self.get(0).ok_or(E::EOF.e())?;
            *self = &self[1..];
            Ok(v)
        }

        fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>> {
            let bits = len * u8::BITS as usize;
            let slice = self.get(..bits).ok_or(E::EOF.e())?;
            *self = &self[bits..];

            let mut vec = vec![0u8; len];
            vec.as_mut_bits().copy_from_bitslice(slice);
            Ok(vec)
        }

        fn read_zeros(&mut self, max: usize) -> Result<usize> {
            let zeros = self.leading_zeros();
            if zeros > max {
                Err(E::Invalid("zeros").e())
            } else {
                *self = &self[zeros..];
                let next = *self.get(0).ok_or(E::EOF.e())?;
                debug_assert!(next);
                Ok(zeros)
            }
        }
    }

    impl<'a> ReadWith<'a> for BitSliceImpl<'a> {
        fn from_inner(inner: &'a [u8]) -> Self {
            BitSlice::from_slice(inner)
        }
    }
}

#[derive(Debug)]
pub struct DeVec<'a> {
    words: Box<[Word]>,
    read: usize,
    bytes: usize,
    _spooky: PhantomData<&'a ()>, // To be compatible with BitVec.
}

impl<'a> DeVec<'a> {
    /// Extra [`Word`]s appended to the end of the input to make deserialization faster.
    /// 1 for peek_reserved_bits and another for read_zeros (which calls peek_reserved_bits).
    const PADDING: usize = 2;

    fn peek_reserved_bits(&self, bits: usize) -> Word {
        debug_assert!(bits >= 1 && bits <= WORD_BITS);
        let bit_index = self.read;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        let a = self.words[index] >> bit_remainder;
        let b = self.words[index + 1]
            .checked_shl((WORD_BITS - bit_remainder) as u32)
            .unwrap_or(0);

        // Clear bits at end (don't need to do in ser because bits at end are zeroed).
        let extra_bits = WORD_BITS - bits;
        ((a | b) << extra_bits) >> extra_bits
    }

    fn read_reserved_bits(&mut self, bits: usize) -> Word {
        let v = self.peek_reserved_bits(bits);
        self.read += bits;
        v
    }

    /// Faster [`Self::reserve`] that can elide bounds checks for `bits` in 1..=64.
    fn reserve_1_to_64(&self, bits: usize) -> Result<()> {
        debug_assert!(bits >= 1 && bits <= WORD_BITS);

        let read = self.read / WORD_BITS;
        let len = self.words.len();
        if read + 1 >= len {
            // TODO hint as unlikely.
            Err(E::EOF.e())
        } else {
            Ok(())
        }
    }

    fn reserve(&self, bits: usize) -> Result<()> {
        let read = self.read + bits + WORD_BITS; // Don't add 1 since can't read 0.
        let len = self.words.len() * WORD_BITS;
        if read > len {
            // TODO hint as unlikely.
            Err(E::EOF.e())
        } else {
            Ok(())
        }
    }
}

impl<'a> Read for DeVec<'a> {
    fn finish(self) -> Result<()> {
        let bytes_read = div_ceil(self.read, u8::BITS as usize);
        let index = self.read / WORD_BITS;
        let bits_written = self.read % WORD_BITS;

        if bits_written != 0 && self.words[index] & !((1 << bits_written) - 1) != 0 {
            return Err(E::ExpectedEOF.e());
        }

        if bytes_read < self.bytes {
            Err(E::ExpectedEOF.e())
        } else if bytes_read > self.bytes {
            // It is possible that we read more bytes than we have (bytes are rounded up to words).
            // We don't check this while deserializing to avoid degrading performance.
            Err(E::EOF.e())
        } else {
            Ok(())
        }
    }

    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        self.reserve_1_to_64(bits)?;
        Ok(self.read_reserved_bits(bits))
    }

    fn read_bit(&mut self) -> Result<bool> {
        self.reserve_1_to_64(1)?;

        let bit_index = self.read;
        self.read += 1;

        let index = bit_index / WORD_BITS;
        let bit_remainder = bit_index % WORD_BITS;

        Ok((self.words[index] & (1 << bit_remainder)) != 0)
    }

    // TODO optimize more.
    fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>> {
        let bits = len * u8::BITS as usize;
        self.reserve(bits)?;
        let mut vec = Vec::with_capacity(len);

        // Read whole words at a time.
        vec.extend(
            (0..len / WORD_BYTES).flat_map(|_| self.read_reserved_bits(WORD_BITS).to_le_bytes()),
        );

        // Read remaining bytes (could try calling read_reserved_bits once with more than 8 bits).
        vec.extend((0..len % WORD_BYTES).map(|_| self.read_reserved_bits(u8::BITS as usize) as u8));

        Ok(vec)
    }

    fn read_zeros(&mut self, max: usize) -> Result<usize> {
        let max_plus_one = max + 1;
        self.reserve_1_to_64(max_plus_one)?;

        let zeros = self.peek_reserved_bits(max_plus_one).trailing_zeros() as usize;
        self.read += zeros;
        if zeros < max_plus_one {
            Ok(zeros)
        } else {
            Err(E::Invalid("zeros").e())
        }
    }
}

impl<'a> ReadWith<'a> for DeVec<'a> {
    fn from_inner(inner: &'a [u8]) -> Self {
        // u8s rounded up to u64s plus 1 u64 padding.
        let capacity = div_ceil(inner.len(), WORD_BYTES) + Self::PADDING;
        let mut vec = Vec::with_capacity(capacity);

        // Fast hot loop (would be nicer with array_chunks, but that requires nightly).
        let chunks = inner.chunks_exact(WORD_BYTES);
        let remainder = chunks.remainder();
        vec.extend(chunks.map(|chunk| {
            let chunk: &[u8; 8] = chunk.try_into().unwrap();
            Word::from_le_bytes(*chunk)
        }));

        // Remaining bytes.
        if !remainder.is_empty() {
            vec.push(u64::from_le_bytes(array::from_fn(|i| {
                remainder.get(i).copied().unwrap_or_default()
            })));
        }

        // Padding so peek_reserved_bits doesn't ever go out of bounds.
        vec.extend([0; Self::PADDING]);
        debug_assert_eq!(vec.len(), capacity);

        Self {
            words: vec.into_boxed_slice(),
            read: 0,
            bytes: inner.len(),
            _spooky: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::{black_box, Bencher};

    #[bench]
    fn bench_de_vec_from_inner(b: &mut Bencher) {
        let bytes = vec![123u8; 6659];
        b.iter(|| {
            let bytes = black_box(bytes.as_slice());
            black_box(DeVec::from_inner(bytes))
        });
    }

    // How many times each benchmark calls the function.
    const TIMES: usize = 1000;

    // TODO figure out lifetimes to make this a function.
    macro_rules! bench_read_bit {
        ($T:ty, $b:ident) => {
            let v = vec![123u8; div_ceil(TIMES, u8::BITS as usize)];
            $b.iter(|| {
                let mut data = <$T>::from_inner(black_box(&v));
                for _ in 0..black_box(TIMES) {
                    black_box(data.read_bit().unwrap());
                }
            });
        };
    }

    // TODO figure out lifetimes to make this a function.
    macro_rules! bench_read_bits {
        ($T:ty, $b:ident, $bits:expr) => {
            let v = vec![123u8; div_ceil($bits * TIMES, u8::BITS as usize)];
            $b.iter(|| {
                let mut data = <$T>::from_inner(black_box(&v));
                for _ in 0..black_box(TIMES) {
                    black_box(data.read_bits($bits).unwrap());
                }
            });
        };
    }

    // TODO figure out lifetimes to make this a function.
    macro_rules! bench_read_bytes {
        ($T:ty, $b:ident, $bytes:expr) => {
            let v = vec![123u8; $bytes * TIMES];
            $b.iter(|| {
                let mut data = <$T>::from_inner(black_box(&v));
                for _ in 0..black_box(TIMES) {
                    black_box(data.read_bytes($bytes).unwrap());
                }
            });
        };
    }

    // TODO figure out lifetimes to make this a function.
    macro_rules! bench_read_zeros {
        ($T:ty, $b:ident) => {
            let v = vec![1 << 7; TIMES];
            $b.iter(|| {
                let mut data = <$T>::from_inner(black_box(&v));
                for _ in 0..black_box(TIMES) {
                    black_box(data.read_zeros(black_box(WORD_BITS - 1)).unwrap());
                    data.read_bit().unwrap();
                }
            });
        };
    }

    #[bench]
    fn bench_bit_read_bit1(b: &mut Bencher) {
        bench_read_bit!(BitSliceImpl, b);
    }

    #[bench]
    fn bench_de_vec_read_bit1(b: &mut Bencher) {
        bench_read_bit!(DeVec, b);
    }

    #[bench]
    fn bench_bit_slice_read_bits_5(b: &mut Bencher) {
        bench_read_bits!(BitSliceImpl, b, 5);
    }

    #[bench]
    fn bench_de_vec_read_bits_5(b: &mut Bencher) {
        bench_read_bits!(DeVec, b, 5);
    }

    #[bench]
    fn bench_bit_slice_read_bits_41(b: &mut Bencher) {
        bench_read_bits!(BitSliceImpl, b, 41);
    }

    #[bench]
    fn bench_de_vec_read_bits_41(b: &mut Bencher) {
        bench_read_bits!(DeVec, b, 41);
    }

    #[bench]
    fn bench_bit_slice_read_bytes_5(b: &mut Bencher) {
        bench_read_bytes!(BitSliceImpl, b, 5);
    }

    #[bench]
    fn bench_de_vec_read_bytes_5(b: &mut Bencher) {
        bench_read_bytes!(DeVec, b, 5);
    }

    #[bench]
    fn bench_bit_slice_read_bytes_100(b: &mut Bencher) {
        bench_read_bytes!(BitSliceImpl, b, 100);
    }

    #[bench]
    fn bench_de_vec_read_bytes_100(b: &mut Bencher) {
        bench_read_bytes!(DeVec, b, 100);
    }

    #[bench]
    fn bench_bit_slice_read_bytes_1000(b: &mut Bencher) {
        bench_read_bytes!(BitSliceImpl, b, 1000);
    }

    #[bench]
    fn bench_de_vec_read_bytes_1000(b: &mut Bencher) {
        bench_read_bytes!(DeVec, b, 1000);
    }

    #[bench]
    fn bench_bit_slice_read_zeros(b: &mut Bencher) {
        bench_read_zeros!(BitSliceImpl, b);
    }

    #[bench]
    fn bench_de_vec_read_zeros(b: &mut Bencher) {
        bench_read_zeros!(DeVec, b);
    }
}
