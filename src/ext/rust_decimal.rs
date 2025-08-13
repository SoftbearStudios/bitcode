use crate::{
    convert::{self, impl_convert, ConvertFrom},
    Decode, Encode,
};
use bytemuck::CheckedBitPattern;
use rust_decimal::Decimal;
type DecimalConversion = ([u8; 12], Flags);

impl ConvertFrom<&Decimal> for DecimalConversion {
    #[inline(always)]
    fn convert_from(value: &Decimal) -> Self {
        let unpacked = value.unpack();
        let bytes = [
            unpacked.lo,
            unpacked.mid,
            unpacked.hi,
        ].map(u32::to_le_bytes);
        (
            core::array::from_fn(|i| bytes[i / 4][i % 4]),
            Flags::new(unpacked.scale, unpacked.negative),
        )
    }
}

impl ConvertFrom<DecimalConversion> for Decimal {
    #[inline(always)]
    fn convert_from(value: DecimalConversion) -> Self {
        let scale = value.1.scale();
        // Should make Decimal::from_parts faster, once it can be inlined,
        // since it can skip division.
        // Safety: impl CheckedBitPattern for Flags guarantees this.
        unsafe {
            if scale > 28 {
                core::hint::unreachable_unchecked();
            }
        }
        let [lo, mid, hi] = core::array::from_fn(|i| u32::from_le_bytes(value.0[i * 4..(i + 1) * 4].try_into().unwrap()));
        let mut ret = Self::from_parts(lo, mid, hi, false, scale);
        ret.set_sign_negative(value.1.negative());
        ret
    }
}

impl_convert!(Decimal, DecimalConversion);

impl ConvertFrom<&Flags> for u8 {
    #[inline(always)]
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
    use std::str::FromStr;

    #[test]
    fn rust_decimal() {
        let vs = [
            Decimal::from(0),
            Decimal::from_f64_retain(-0f64).unwrap(),
            Decimal::from(-1),
            Decimal::from(1) / Decimal::from(2),
            Decimal::from(1),
            Decimal::from(999999999999999999u64),
            Decimal::from_str("3.100").unwrap(),
        ];
        for v in vs {
            let d = decode::<Decimal>(&encode(&v)).unwrap();
            assert_eq!(d, v);
            assert_eq!(d.is_sign_negative(), v.is_sign_negative());
            assert_eq!(d.scale(), v.scale());
        }
    }

    use alloc::vec::Vec;
    fn bench_data() -> Vec<Decimal> {
        crate::random_data(1000)
            .into_iter()
            .map(|(n, s): (i64, u32)| {
                Decimal::new(n, s % (Decimal::MAX_SCALE + 1))
            })
            .collect()
    }
    crate::bench_encode_decode!(decimal_vec: Vec<_>);
}
