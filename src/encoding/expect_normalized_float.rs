use crate::code::{Decode, Encode};
use crate::encoding::prelude::*;
use crate::encoding::{Fixed, Gamma};

#[derive(Copy, Clone)]
pub struct ExpectNormalizedFloat;

// Should be a power of 2 minus 1 for max efficiency. More than 15 doesn't improve good case much
// but makes bad case at least 2 bits more.
const MAX_GAMMA_EXP: u32 = 15;

macro_rules! impl_float {
    ($write:ident, $read:ident, $t:ty, $i: ty, $mantissa:literal, $exp_bias: literal, $exp_type:ident) => {
        fn $write(self, writer: &mut impl Write, v: $t) {
            let mantissa_bits = $mantissa as usize;
            let exp_bias = $exp_bias as u32;
            let sign_bit = 1 << (<$i>::BITS - 1);

            let bits = v.to_bits();
            let sign = bits & sign_bit != 0;
            let bits_without_sign = bits & !sign_bit;
            let exp = (bits_without_sign >> mantissa_bits) as u32;
            let gamma_exp = (exp_bias - 1).wrapping_sub(exp) as $exp_type;

            if !sign && gamma_exp < MAX_GAMMA_EXP as $exp_type {
                let mantissa = bits as $i & !(<$i>::MAX << mantissa_bits);

                (gamma_exp).encode(Gamma, writer).unwrap();
                writer.write_bits(mantissa.into(), mantissa_bits);
            } else {
                #[cold]
                fn cold(writer: &mut impl Write, v: $t) {
                    MAX_GAMMA_EXP.encode(Gamma, writer).unwrap();
                    v.encode(Fixed, writer).unwrap()
                }
                cold(writer, v);
            }
        }

        fn $read(self, reader: &mut impl Read) -> Result<$t> {
            let mantissa_bits = $mantissa as usize;
            let exp_bias = $exp_bias as u32;

            let gamma_exp = $exp_type::decode(Gamma, reader)?;
            if gamma_exp < MAX_GAMMA_EXP as $exp_type {
                let mantissa = reader.read_bits(mantissa_bits)? as $i;
                let exp = (exp_bias - 1) - gamma_exp as u32;

                Ok(<$t>::from_bits(exp as $i << mantissa_bits | mantissa))
            } else {
                #[cold]
                fn cold(reader: &mut impl Read) -> Result<$t> {
                    <$t>::decode(Fixed, reader)
                }
                cold(reader)
            }
        }
    }
}

impl Encoding for ExpectNormalizedFloat {
    impl_float!(write_f32, read_f32, f32, u32, 23, 127, u8);
    impl_float!(write_f64, read_f64, f64, u64, 52, 1023, u16);
}

#[cfg(all(test, debug_assertions, not(miri)))]
mod tests {
    macro_rules! impl_test {
        ($t:ty, $i:ty) => {
            use crate::encoding::expect_normalized_float::*;
            use crate::encoding::prelude::test_prelude::*;
            use rand::{Rng, SeedableRng};

            fn t(value: $t) {
                #[derive(Copy, Clone, Debug, Encode, Decode)]
                struct ExactBits(#[bitcode_hint(expected_range = "0.0..1.0")] $t);

                impl PartialEq for ExactBits {
                    fn eq(&self, other: &Self) -> bool {
                        self.0.to_bits() == other.0.to_bits()
                    }
                }
                test_encoding(ExpectNormalizedFloat, ExactBits(value));
            }

            #[test]
            fn test_random() {
                let mut rng = rand_chacha::ChaCha20Rng::from_seed(Default::default());
                for _ in 0..100000 {
                    let f = <$t>::from_bits(rng.gen::<$i>());
                    t(f)
                }
            }

            #[test]
            fn test2() {
                t(0.0);
                t(0.5);
                t(1.0);
                t(-1.0);
                t(<$t>::INFINITY);
                t(<$t>::NEG_INFINITY);
                t(<$t>::NAN);
                t(0.0000000000001);

                fn normalized_floats(n: usize) -> impl Iterator<Item = $t> {
                    let scale = 1.0 / n as $t;
                    (0..n).map(move |i| i as $t * scale)
                }

                fn normalized_float_bits(n: usize) -> $t {
                    let mut buffer = crate::word_buffer::WordBuffer::default();
                    buffer.start_write();
                    for v in normalized_floats(n) {
                        v.encode(ExpectNormalizedFloat, &mut buffer).unwrap();
                    }

                    let bytes = buffer.finish_write().to_vec();
                    buffer.start_read(&bytes);
                    for v in normalized_floats(n) {
                        let decoded = <$t>::decode(ExpectNormalizedFloat, &mut buffer).unwrap();
                        assert_eq!(decoded, v);
                    }
                    buffer.finish_read().unwrap();

                    (bytes.len() * u8::BITS as usize) as $t / n as $t
                }

                if <$i>::BITS == 32 {
                    assert!((25.26..25.5).contains(&normalized_float_bits(1 << 12)));
                    // panic!("bits {}", normalized_float_bits(6000000)); // bits 25.265963
                } else {
                    assert!((54.26..54.5).contains(&normalized_float_bits(1 << 12)));
                    // panic!("bits {}", normalized_float_bits(6000000)); // bits 54.2660546
                }
            }
        };
    }

    mod f32 {
        impl_test!(f32, u32);
    }

    mod f64 {
        impl_test!(f64, u64);
    }
}
