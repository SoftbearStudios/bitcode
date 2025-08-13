use crate::{
    convert::{impl_convert, ConvertFrom},
    int::ranged_int,
};
use rust_decimal::Decimal;

type Mantissa = [u8; 12];
ranged_int!(Scale, u8, 0, Decimal::MAX_SCALE as u8);
type DecimalEncode = (Mantissa, bool, u8);
type DecimalDecode = (Mantissa, bool, Scale);

impl ConvertFrom<&Decimal> for DecimalEncode {
    #[inline(always)]
    fn convert_from(value: &Decimal) -> Self {
        let unpacked = value.unpack();
        let [a0, a1, a2, a3] = unpacked.lo.to_le_bytes();
        let [b0, b1, b2, b3] = unpacked.mid.to_le_bytes();
        let [c0, c1, c2, c3] = unpacked.hi.to_le_bytes();
        (
            [a0, a1, a2, a3, b0, b1, b2, b3, c0, c1, c2, c3],
            unpacked.negative,
            unpacked.scale as u8,
        )
    }
}

impl ConvertFrom<DecimalDecode> for Decimal {
    #[inline(always)]
    fn convert_from(value: DecimalDecode) -> Self {
        let [a0, a1, a2, a3, b0, b1, b2, b3, c0, c1, c2, c3] = value.0;
        let lo = u32::from_le_bytes([a0, a1, a2, a3]);
        let mid = u32::from_le_bytes([b0, b1, b2, b3]);
        let hi = u32::from_le_bytes([c0, c1, c2, c3]);
        let mut ret = Self::from_parts(lo, mid, hi, false, value.2.into_inner() as u32);
        ret.set_sign_negative(value.1);
        ret
    }
}

impl_convert!(Decimal, DecimalEncode, DecimalDecode);

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn rust_decimal() {
        assert!(Decimal::MAX_SCALE <= u8::MAX as u32);

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

        assert!(crate::decode::<Decimal>(&crate::encode(&([42u8; 12], false, 28u8))).is_ok());
        assert!(crate::decode::<Decimal>(&crate::encode(&([42u8; 12], false, 29u8))).is_err());
    }

    use alloc::vec::Vec;
    fn bench_data() -> Vec<Decimal> {
        crate::random_data(1000)
            .into_iter()
            .map(|(n, s): (i64, u32)| Decimal::new(n, s % (Decimal::MAX_SCALE + 1)))
            .collect()
    }
    crate::bench_encode_decode!(decimal_vec: Vec<_>);
}
