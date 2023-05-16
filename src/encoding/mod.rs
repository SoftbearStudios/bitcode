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
    fn is_fixed(self) -> bool {
        false
    }

    fn zigzag(self) -> bool {
        false
    }

    #[inline(always)]
    fn write_word<const BITS: usize>(self, writer: &mut impl Write, word: Word) {
        writer.write_bits(word, BITS);
    }

    #[inline(always)]
    fn read_word<const BITS: usize>(self, reader: &mut impl Read) -> Result<Word> {
        reader.read_bits(BITS)
    }

    #[inline(always)]
    fn write_f32(self, writer: &mut impl Write, v: f32) {
        v.to_bits().encode(Fixed, writer).unwrap()
    }

    #[inline(always)]
    fn read_f32(self, reader: &mut impl Read) -> Result<f32> {
        Ok(f32::from_bits(Decode::decode(Fixed, reader)?))
    }

    #[inline(always)]
    fn write_f64(self, writer: &mut impl Write, v: f64) {
        v.to_bits().encode(Fixed, writer).unwrap()
    }

    #[inline(always)]
    fn read_f64(self, reader: &mut impl Read) -> Result<f64> {
        Ok(f64::from_bits(Decode::decode(Fixed, reader)?))
    }

    #[inline(always)]
    fn write_str(self, writer: &mut impl Write, v: &str) {
        v.len().encode(Gamma, writer).unwrap();
        writer.write_bytes(v.as_bytes());
    }

    #[inline(always)]
    fn read_str(self, reader: &mut impl Read) -> Result<&str> {
        let len = usize::decode(Gamma, reader)?;
        let bytes = reader.read_bytes(len)?;
        std::str::from_utf8(bytes).map_err(|_| E::Invalid("utf8").e())
    }
}

#[derive(Copy, Clone)]
pub struct Fixed;

impl Encoding for Fixed {
    fn is_fixed(self) -> bool {
        true
    }
}
