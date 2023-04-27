use super::prelude::*;

#[derive(Copy, Clone)]
pub struct Gamma;
impl Encoding for Gamma {
    fn zigzag(self) -> bool {
        true
    }

    fn write_word(self, writer: &mut impl Write, word: Word, bits: usize) {
        debug_assert!(bits <= WORD_BITS);
        if bits != WORD_BITS {
            debug_assert_eq!(word, word & ((1 << bits) - 1));
        }

        // https://en.wikipedia.org/wiki/Elias_gamma_coding
        // Gamma can't encode 0 so add 1.
        if word < u32::MAX as u64 {
            let v = word + 1;

            let zero_bits = ilog2_u64(v) as usize;
            let integer_bits = zero_bits + 1;
            let gamma_bits = integer_bits + zero_bits;

            // Rotate bits mod `integer_bits` instead of reversing since it's faster.
            // 00001bbb -> 0000bbb1
            let rotated = ((v as u64) << 1 & !(1 << integer_bits)) | 1;
            let gamma = rotated << zero_bits;
            writer.write_bits(gamma, gamma_bits);
        } else {
            // `zero_bits` + `integer_bits` won't fit in a single call to write_bits.
            // This only happens if v is larger than u32::MAX so we mark it as #[cold].
            #[cold]
            fn slow(writer: &mut impl Write, word: Word) {
                // Special case u64::MAX as 64 zeros.
                if word == Word::MAX {
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
            slow(writer, word);
        }
    }

    #[inline] // TODO is required?.
    fn read_word(self, reader: &mut impl Read, bits: usize) -> Result<Word> {
        debug_assert!((1..=WORD_BITS).contains(&bits));
        let zero_bits = reader.peek_bits()?.trailing_zeros() as usize;
        reader.advance(zero_bits)?;

        // 64 zeros is u64::MAX is special case.
        if zero_bits == Word::BITS as usize {
            return Ok(Word::MAX);
        }

        let integer_bits = zero_bits + 1;
        let rotated = reader.read_bits(integer_bits)?;

        // Rotate bits mod `integer_bits` instead of reversing since it's faster.
        // 0000bbb1 -> 00001bbb
        let v = (rotated as u64 >> 1) | (1 << (integer_bits - 1));

        // Gamma can't encode 0 so sub 1 (see Gamma::write_word for more details).
        let v = v - 1;
        if bits < 64 && v >= (1 << bits) {
            // Could remove the possibility of an error by making uN::MAX encode as N zeros.
            // This might slow down encode in the common case though.
            Err(E::Invalid("gamma").e())
        } else {
            Ok(v)
        }
    }
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
