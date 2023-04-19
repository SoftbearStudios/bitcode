use crate::word::Word;

/// Abstracts over writing bits to a buffer.
pub trait Write {
    /// Clears the buffer.
    fn start_write(&mut self);
    /// Returns the written bytes.
    fn finish_write(&mut self) -> &[u8];
    /// Writes up to 64 bits. The index of `word`'s most significant 1 must be < `bits`.
    /// `bits` must be in range `0..=64`.
    fn write_bits(&mut self, word: Word, bits: usize);
    /// Writes a bit.
    fn write_bit(&mut self, v: bool);
    /// Writes bytes.
    fn write_bytes(&mut self, bytes: &[u8]);
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;
    use crate::bit_buffer::BitBuffer;
    use crate::word_buffer::WordBuffer;
    use paste::paste;
    use test::{black_box, Bencher};

    // How many times each benchmark calls the function.
    const TIMES: usize = 1000;

    #[bench]
    fn bench_vec(b: &mut Bencher) {
        let mut vec = vec![];
        b.iter(|| {
            let vec = black_box(&mut vec);
            vec.clear();
            for _ in 0..TIMES {
                vec.push(black_box(0b10101u8))
            }
            black_box(vec);
        });
    }

    fn bench_write_bit<T: Write + Default>(b: &mut Bencher) {
        let mut buf = T::default();
        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_write();
            for _ in 0..TIMES {
                buf.write_bit(black_box(true))
            }
        });
    }

    fn bench_write_bytes<T: Write + Default>(b: &mut Bencher, bytes: usize) {
        let v = vec![123u8; bytes];
        let mut buf = T::default();
        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_write();
            buf.write_bit(true); // Make write_bytes unaligned.
            for _ in 0..TIMES {
                buf.write_bytes(black_box(v.as_slice()))
            }
        });
    }

    fn bench_write_bits<T: Write + Default>(b: &mut Bencher, bits: usize) {
        let v = Word::MAX >> (Word::BITS as usize - bits);
        let mut buf = T::default();
        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_write();
            for _ in 0..TIMES {
                buf.write_bits(black_box(v), black_box(bits))
            }
        });
    }

    macro_rules! bench_write_bits {
        ($name:ident, $T:ty, $n:literal) => {
            paste! {
                #[bench]
                fn [<bench_ $name _write_bits_ $n>](b: &mut Bencher) {
                    bench_write_bits::<$T>(b, $n);
                }
            }
        };
    }

    macro_rules! bench_write_bytes {
        ($name:ident, $T:ty, $n:literal) => {
            paste! {
                #[bench]
                fn [<bench_ $name _write_bytes_ $n>](b: &mut Bencher) {
                    bench_write_bytes::<$T>(b, $n);
                }
            }
        };
    }

    macro_rules! bench_write {
        ($name:ident, $T:ty) => {
            paste! {
                #[bench]
                fn [<bench_ $name _write_bit1>](b: &mut Bencher) {
                    bench_write_bit::<$T>(b);
                }
            }

            bench_write_bits!($name, $T, 5);
            bench_write_bits!($name, $T, 41);
            bench_write_bytes!($name, $T, 1);
            bench_write_bytes!($name, $T, 10);
            bench_write_bytes!($name, $T, 100);
            bench_write_bytes!($name, $T, 1000);
        };
    }

    bench_write!(bit_buffer, BitBuffer);
    bench_write!(word_buffer, WordBuffer);
}
