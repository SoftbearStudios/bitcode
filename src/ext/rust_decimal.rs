use crate::{
    convert::{impl_convert, ConvertFrom},
    int::ranged_int,
};
use rust_decimal::Decimal;

type DecimalEncode = ([u8; 12], u8, bool);
type DecimalDecode = ([u8; 12], Scale, bool);

impl ConvertFrom<&Decimal> for DecimalEncode {
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
            unpacked.scale as u8,
            unpacked.negative,
        )
    }
}

impl ConvertFrom<DecimalDecode> for Decimal {
    #[inline(always)]
    fn convert_from(value: DecimalDecode) -> Self {
        let [lo, mid, hi] = core::array::from_fn(|i| u32::from_le_bytes(value.0[i * 4..(i + 1) * 4].try_into().unwrap()));
        let mut ret = Self::from_parts(lo, mid, hi, false, value.1.into_inner() as u32);
        ret.set_sign_negative(value.2);
        ret
    }
}

impl_convert!(Decimal, DecimalEncode, DecimalDecode);

ranged_int!(Scale, u8, 0, Decimal::MAX_SCALE as u8);

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
