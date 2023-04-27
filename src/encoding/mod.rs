mod expect_normalized_float;
mod expected_range_u64;
mod gamma;
mod prelude;

use crate::Decode;
use crate::Encode;
pub use expect_normalized_float::ExpectNormalizedFloat;
pub use expected_range_u64::ExpectedRangeU64;
pub use gamma::Gamma;
use prelude::*;

pub trait Encoding: Copy {
    fn zigzag(self) -> bool {
        false
    }

    fn write_word(self, writer: &mut impl Write, word: Word, bits: usize) {
        writer.write_bits(word, bits);
    }

    fn read_word(self, reader: &mut impl Read, bits: usize) -> Result<Word> {
        reader.read_bits(bits)
    }

    fn write_f32(self, writer: &mut impl Write, v: f32) {
        v.to_bits().encode(Fixed, writer).unwrap()
    }

    fn read_f32(self, reader: &mut impl Read) -> Result<f32> {
        Ok(f32::from_bits(Decode::decode(Fixed, reader)?))
    }

    fn write_f64(self, writer: &mut impl Write, v: f64) {
        v.to_bits().encode(Fixed, writer).unwrap()
    }

    fn read_f64(self, reader: &mut impl Read) -> Result<f64> {
        Ok(f64::from_bits(Decode::decode(Fixed, reader)?))
    }
}

#[derive(Copy, Clone)]
pub struct Fixed;

impl Encoding for Fixed {}
