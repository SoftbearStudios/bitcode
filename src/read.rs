use crate::word::Word;
use crate::{Result, E};

/// Abstracts over reading bits from a buffer.
pub trait Read {
    /// Copies `bytes` to be read.
    fn start_read(&mut self, bytes: &[u8]);
    /// Check for errors such as Eof and ExpectedEof
    fn finish_read(&self) -> Result<()>;
    /// Overrides decoding errors with Eof since the reader might allow reading slightly past the
    /// end. Only WordBuffer currently does this.
    fn finish_read_with_result<T>(&self, decode_result: Result<T>) -> Result<T> {
        let finish_result = self.finish_read();
        if let Err(e) = &finish_result {
            if e.same(&E::Eof.e()) {
                return Err(E::Eof.e());
            }
        }
        let t = decode_result?;
        finish_result?;
        Ok(t)
    }
    /// Advances any amount of bits. May or may not return EOF.
    fn advance(&mut self, bits: usize) -> Result<()>;
    /// Peeks 64 bits without reading them. Bits after EOF are zeroed.
    fn peek_bits(&mut self) -> Result<Word>;
    // Reads 1 bit.
    fn read_bit(&mut self) -> Result<bool>;
    /// Reads up to 64 bits. `bits` must be in range `1..=64`.
    fn read_bits(&mut self, bits: usize) -> Result<Word>;
    /// Reads `len` bytes.
    fn read_bytes(&mut self, len: usize) -> Result<&[u8]>;
    /// Ensures that at least `bits` remain. Never underreports remaining bits.
    fn reserve_bits(&self, bits: usize) -> Result<()>;
}

#[cfg(all(test, not(miri)))]
mod tests {
    use crate::bit_buffer::BitBuffer;
    use crate::nightly::div_ceil;
    use crate::read::Read;
    use crate::word_buffer::WordBuffer;
    use paste::paste;
    use test::{black_box, Bencher};

    fn bench_start_read<T: Read + Default>(b: &mut Bencher) {
        let bytes = vec![123u8; 6659];
        let mut buf = T::default();

        b.iter(|| {
            let bytes = black_box(bytes.as_slice());
            buf.start_read(bytes);
            black_box(&mut buf);
        });
    }

    // How many times each benchmark calls the function.
    const TIMES: usize = 1000;

    fn bench_read_bit<T: Read + Default>(b: &mut Bencher) {
        let bytes = vec![123u8; div_ceil(TIMES, u8::BITS as usize)];
        let mut buf = T::default();
        buf.start_read(&bytes);

        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_read(black_box(&bytes));
            for _ in 0..black_box(TIMES) {
                black_box(buf.read_bit().unwrap());
            }
        });
    }

    fn bench_read_bits<T: Read + Default>(b: &mut Bencher, bits: usize) {
        let bytes = vec![123u8; div_ceil(bits * TIMES, u8::BITS as usize)];
        let mut buf = T::default();
        buf.start_read(&bytes);

        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_read(black_box(&bytes));
            for _ in 0..black_box(TIMES) {
                black_box(buf.read_bits(bits).unwrap());
            }
        });
    }

    fn bench_read_bytes<T: Read + Default>(b: &mut Bencher, byte_count: usize) {
        let bytes = vec![123u8; byte_count * TIMES + 1];
        let mut buf = T::default();
        buf.start_read(&bytes);

        b.iter(|| {
            let buf = black_box(&mut buf);
            buf.start_read(black_box(&bytes));
            buf.read_bit().unwrap(); // Make read_bytes unaligned.
            for _ in 0..black_box(TIMES) {
                black_box(buf.read_bytes(byte_count).unwrap());
            }
        });
    }

    macro_rules! bench_read_bits {
        ($name:ident, $T:ty, $n:literal) => {
            paste! {
                #[bench]
                fn [<bench_ $name _read_bits_ $n>](b: &mut Bencher) {
                    bench_read_bits::<$T>(b, $n);
                }
            }
        };
    }

    macro_rules! bench_read_bytes {
        ($name:ident, $T:ty, $n:literal) => {
            paste! {
                #[bench]
                fn [<bench_ $name _read_bytes_ $n>](b: &mut Bencher) {
                    bench_read_bytes::<$T>(b, $n);
                }
            }
        };
    }

    macro_rules! bench_read {
        ($name:ident, $T:ty) => {
            paste! {
                #[bench]
                fn [<bench_ $name _copy_from_slice>](b: &mut Bencher) {
                    bench_start_read::<$T>(b);
                }

                #[bench]
                fn [<bench_ $name _read_bit1>](b: &mut Bencher) {
                    bench_read_bit::<$T>(b);
                }
            }

            bench_read_bits!($name, $T, 5);
            bench_read_bits!($name, $T, 41);
            bench_read_bytes!($name, $T, 1);
            bench_read_bytes!($name, $T, 10);
            bench_read_bytes!($name, $T, 100);
            bench_read_bytes!($name, $T, 1000);
        };
    }

    bench_read!(bit_buffer, BitBuffer);
    bench_read!(word_buffer, WordBuffer);
}
