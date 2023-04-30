use super::prelude::*;

#[derive(Copy, Clone)]
pub struct Gamma;
impl Encoding for Gamma {
    fn zigzag(self) -> bool {
        true
    }

    #[inline]
    fn write_word<const BITS: usize>(self, writer: &mut impl Write, word: Word) {
        debug_assert!(BITS <= WORD_BITS);
        if BITS != WORD_BITS {
            debug_assert_eq!(word, word & ((1 << BITS) - 1));
        }

        if word < u32::MAX as u64 {
            // https://en.wikipedia.org/wiki/Elias_gamma_coding
            // Gamma can't encode 0 so add 1.
            let v = word + 1;

            let zero_bits = ilog2_u64(v) as usize;
            let integer_bits = zero_bits + 1;
            let gamma_bits = zero_bits + integer_bits;

            // Rotate bits mod `integer_bits` instead of reversing since it's faster.
            // 00001bbb -> 0000bbb1
            let rotated = (v << 1 & !(1 << integer_bits)) | 1;
            let gamma = rotated << zero_bits;

            writer.write_bits(gamma, gamma_bits);
        } else {
            // The representation is > 64 bits or or we want to write 64 zeros since it's u64::MAX.
            #[cold]
            fn slow<const BITS: usize>(writer: &mut impl Write, word: Word) {
                // Special case u64::MAX as 64 zeros.
                if BITS == WORD_BITS && word == u64::MAX {
                    writer.write_bits(0, WORD_BITS);
                    return;
                }
                let v = word + 1;

                let zero_bits = ilog2_u64(v) as usize;
                writer.write_bits(0, zero_bits);

                let integer_bits = zero_bits + 1;
                let rotated = (v << 1 & !(1 << integer_bits)) | 1;
                writer.write_bits(rotated, integer_bits);
            }
            slow::<BITS>(writer, word);
        }
    }

    #[inline] // TODO is required?.
    fn read_word<const BITS: usize>(self, reader: &mut impl Read) -> Result<Word> {
        debug_assert!((1..=WORD_BITS).contains(&BITS));

        let peek = reader.peek_bits()?;
        let zero_bits = peek.trailing_zeros() as usize;

        if zero_bits < u32::BITS as usize {
            let integer_bits = zero_bits + 1;
            let gamma_bits = zero_bits + integer_bits;
            reader.advance(gamma_bits);

            let rotated = peek >> zero_bits & ((1 << integer_bits) - 1);

            // Rotate bits mod `integer_bits` instead of reversing since it's faster.
            // 0000bbb1 -> 00001bbb
            let v = (rotated as u64 >> 1) | (1 << (integer_bits - 1));

            // Gamma can't encode 0 so sub 1.
            let v = v - 1;
            Ok(v)
        } else {
            // The representation is > 64 bits or or we want to read 64 zeros since it's u64::MAX.
            #[cold]
            fn slow<const BITS: usize>(reader: &mut impl Read) -> Result<Word> {
                let zero_bits = reader.peek_bits()?.trailing_zeros() as usize;
                reader.advance(zero_bits);

                // u64::MAX is special cased as 64 zeros.
                if zero_bits == WORD_BITS {
                    return Ok(if BITS < WORD_BITS { 0 } else { Word::MAX });
                }

                let integer_bits = zero_bits + 1;
                let rotated = reader.read_bits(integer_bits)?;

                let v = (rotated as u64 >> 1) | (1 << (integer_bits - 1));
                let v = v - 1;

                // Mask to valid range.
                Ok(v & (u64::MAX >> (64 - BITS)))
            }
            slow::<BITS>(reader)
        }
    }
}

#[cfg(test)]
mod benches {
    use crate::encoding::prelude::bench_prelude::*;
    use rand::prelude::*;

    type Int = u8;
    fn dataset() -> Vec<Int> {
        let mut rng = rand_chacha::ChaCha20Rng::from_seed(Default::default());
        (0..1000).map(|_| (rng.gen::<u8>() / 2) as Int).collect()
    }

    bench_encoding!(super::Gamma, dataset);
}

#[cfg(all(test, debug_assertions, not(miri)))]
mod tests {
    use super::*;
    use crate::encoding::prelude::test_prelude::*;

    #[test]
    fn test() {
        fn t<V: Encode + Decode + Copy + Debug + PartialEq>(value: V) {
            test_encoding(Gamma, value)
        }

        for i in 0..u8::MAX {
            t(i);
        }

        t(u16::MAX);
        t(u32::MAX);
        t(u64::MAX);

        t(-1i8);
        t(-1i16);
        t(-1i32);
        t(-1i64);

        #[derive(Debug, PartialEq, Encode, Decode)]
        struct GammaInt<T>(#[bitcode_hint(gamma)] T);

        for i in -7..=7i64 {
            // Zig-zag means that low magnitude signed ints are under one byte.
            assert_eq!(bitcode::encode(&GammaInt(i)).unwrap().len(), 1);
        }
    }
}
