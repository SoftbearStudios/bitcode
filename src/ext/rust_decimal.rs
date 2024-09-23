use crate::{
    convert::{self, impl_convert, ConvertFrom},
    Decode, Encode,
};
use bytemuck::CheckedBitPattern;
use rust_decimal::Decimal;
type DecimalConversion = (u32, u32, u32, Flags);

impl ConvertFrom<&Decimal> for DecimalConversion {
    fn convert_from(value: &Decimal) -> Self {
        let unpacked = value.unpack();
        (
            unpacked.lo,
            unpacked.mid,
            unpacked.hi,
            Flags::new(unpacked.scale, unpacked.negative),
        )
    }
}

impl ConvertFrom<DecimalConversion> for Decimal {
    fn convert_from(value: DecimalConversion) -> Self {
        let mut ret = Self::from_parts(
            value.0,
            value.1,
            value.2,
            false,
            value.3.scale(),
        );
        ret.set_sign_negative(value.3.negative());
        ret
    }
}

impl_convert!(Decimal, DecimalConversion);

impl ConvertFrom<&Flags> for u8 {
    fn convert_from(flags: &Flags) -> Self {
        flags.0
    }
}

impl Encode for Flags {
    type Encoder = convert::ConvertIntoEncoder<u8>;
}

/// A u8 guaranteed to satisfy (flags >> 1) <= 28. Prevents Decimal::from_parts from misbehaving.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Flags(u8);

impl Flags {
    #[inline(always)]
    fn new(scale: u32, negative: bool) -> Self {
        Self((scale as u8) << 1 | negative as u8)
    }

    #[inline(always)]
    fn scale(&self) -> u32 {
        (self.0 >> 1) as u32
    }

    #[inline(always)]
    fn negative(&self) -> bool {
        self.0 & 1 == 1
    }
}

// Safety: u8 and Flags have the same layout since Flags is #[repr(transparent)].
unsafe impl CheckedBitPattern for Flags {
    type Bits = u8;
    #[inline(always)]
    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        (*bits >> 1) <= 28
    }
}

impl<'a> Decode<'a> for Flags {
    type Decoder = crate::int::CheckedIntDecoder<'a, Flags, u8>;
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use rust_decimal::Decimal;

    #[test]
    fn rust_decimal() {
        let vs = [
            Decimal::from(0),
            Decimal::from_f64_retain(-0f64).unwrap(),
            Decimal::from(-1),
            Decimal::from(1) / Decimal::from(2),
            Decimal::from(1),
            Decimal::from(999999999999999999u64),
        ];
        for v in vs {
            let d = decode::<Decimal>(&encode(&v)).unwrap();
            assert_eq!(d, v);
            assert_eq!(d.is_sign_negative(), v.is_sign_negative());
        }
    }
}
