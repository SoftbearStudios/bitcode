use jiff::tz::Offset;

use crate::{
    convert::ConvertFrom,
    error::error,
    int::ranged_int,
    try_convert::{impl_try_convert, TryConvertFrom},
};

impl_try_convert!(Offset, OffsetEncoder, OffsetDecoder);

ranged_int!(OffsetDecoder, i32, -93599, 93599);

pub(super) type OffsetEncoder = i32;

impl ConvertFrom<&Offset> for OffsetEncoder {
    #[inline(always)]
    fn convert_from(value: &Offset) -> Self {
        value.seconds()
    }
}

impl TryConvertFrom<OffsetDecoder> for Offset {
    #[inline(always)]
    fn try_convert_from(value: OffsetDecoder) -> Result<Self, crate::Error> {
        Offset::from_seconds(value.into_inner()).map_err(|_| error("Failed to decode offset"))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_offset() {
        let offset = Offset::UTC;
        let bytes = bitcode::encode(&offset);
        let decoded: Offset = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, offset);

        let offset = Offset::from_seconds(93599).unwrap();
        let bytes = bitcode::encode(&offset);
        let decoded: Offset = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, offset);

        let offset = Offset::from_seconds(-93599).unwrap();
        let bytes = bitcode::encode(&offset);
        let decoded: Offset = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, offset);

        let offset = Offset::from_seconds(28800).unwrap();
        let bytes = bitcode::encode(&offset);
        let decoded: Offset = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, offset);

        let offset = Offset::from_seconds(-21600).unwrap();
        let bytes = bitcode::encode(&offset);
        let decoded: Offset = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, offset);

        let bytes = bitcode::encode(&93600);
        let result: Result<Offset, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&-93600);
        let result: Result<Offset, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        assert!(crate::decode::<Offset>(&crate::encode(&Offset::UTC)).is_ok());
    }

    use alloc::vec::Vec;
    use jiff::tz::Offset;

    fn offset_min() -> i32 {
        Offset::MIN.seconds()
    }
    fn offset_max() -> i32 {
        Offset::MAX.seconds()
    }

    fn bench_data() -> Vec<Offset> {
        crate::random_data(1000)
            .into_iter()
            .map(|secs: i32| {
                let secs = secs.clamp(offset_min(), offset_max());
                Offset::from_seconds(secs).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(offset_vec: Vec<_>);
}
