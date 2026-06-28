use crate::{
    convert::ConvertFrom,
    int::ranged_int,
    try_convert::{impl_try_convert, TryConvertFrom},
};
use jiff::Timestamp;

pub type TimestampEncode = (i64, i32);

ranged_int!(UnixSecondTimestamp, i64, -377705023201, 253402207200);

ranged_int!(UnixNanosecondTimestamp, i32, -999_999_999, 999_999_999);

pub type TimestampDecoder = (UnixSecondTimestamp, UnixNanosecondTimestamp);

impl_try_convert!(Timestamp, TimestampEncode, TimestampDecoder);

impl ConvertFrom<&Timestamp> for TimestampEncode {
    #[inline(always)]
    fn convert_from(value: &Timestamp) -> Self {
        (value.as_second(), value.subsec_nanosecond())
    }
}

impl TryConvertFrom<TimestampDecoder> for Timestamp {
    #[inline(always)]
    fn try_convert_from(value: TimestampDecoder) -> Result<Self, crate::Error> {
        Timestamp::new(value.0.into_inner(), value.1.into_inner())
            .map_err(|_| crate::error::error("Failed to decode timestamp"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        let bytes = bitcode::encode(&(0i64, 0i32));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(0, 0).unwrap());

        let bytes = bitcode::encode(&(unix_seconds_min(), 0i32));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(unix_seconds_min(), 0).unwrap());

        let bytes = bitcode::encode(&(unix_seconds_max(), nanos_max()));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(unix_seconds_max(), nanos_max()).unwrap());

        let bytes = bitcode::encode(&(unix_seconds_min() - 1, 0i32));
        let result: Result<Timestamp, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&(unix_seconds_max() + 1, 0i32));
        let result: Result<Timestamp, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&(0i64, -1_000_000_000i32));
        let result: Result<Timestamp, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&(0i64, 1_000_000_000i32));
        let result: Result<Timestamp, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&(unix_seconds_min(), -1i32));
        let result: Result<Timestamp, _> = bitcode::decode(&bytes);
        assert!(result.is_err());

        let bytes = bitcode::encode(&(unix_seconds_min() + 1, -500i32));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(unix_seconds_min() + 1, -500).unwrap());

        let bytes = bitcode::encode(&(1000i64, nanos_min()));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(1000, nanos_min()).unwrap());

        let bytes = bitcode::encode(&(unix_seconds_min(), 500i32));
        let ts: Timestamp = bitcode::decode(&bytes).unwrap();
        assert_eq!(ts, Timestamp::new(unix_seconds_min(), 500).unwrap());
    }

    use alloc::vec::Vec;
    use jiff::Timestamp;

    fn unix_seconds_min() -> i64 {
        Timestamp::MIN.as_second()
    }
    fn unix_seconds_max() -> i64 {
        Timestamp::MAX.as_second()
    }
    fn nanos_min() -> i32 {
        Timestamp::MIN.subsec_nanosecond()
    }
    fn nanos_max() -> i32 {
        Timestamp::MAX.subsec_nanosecond()
    }

    fn bench_data() -> Vec<Timestamp> {
        crate::random_data(1000)
            .into_iter()
            .map(|(s, n): (i64, i32)| {
                Timestamp::new(s % unix_seconds_max(), n % nanos_max()).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(duration_vec: Vec<_>);
}
