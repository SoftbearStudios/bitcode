use crate::convert::{impl_convert, ConvertFrom};
use crate::int::ranged_int;
use time::Time;

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 999_999_999);

type TimeEncode = (u8, u8, u8, u32);
type TimeDecode = (Hour, Minute, Second, Nanosecond);
impl_convert!(Time, TimeEncode, TimeDecode);

impl ConvertFrom<&Time> for TimeEncode {
    #[inline(always)]
    fn convert_from(value: &Time) -> Self {
        value.as_hms_nano()
    }
}

impl ConvertFrom<TimeDecode> for Time {
    #[inline(always)]
    fn convert_from(value: TimeDecode) -> Self {
        Time::from_hms_nano(
            value.0.into_inner(),
            value.1.into_inner(),
            value.2.into_inner(),
            value.3.into_inner(),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        assert!(crate::decode::<Time>(&crate::encode(
            &Time::from_hms_nano(23, 59, 59, 999_999_999).unwrap()
        ))
        .is_ok());
        assert!(crate::decode::<Time>(&crate::encode(&(23u8, 59u8, 59u8, 999_999_999u32))).is_ok());
        assert!(
            crate::decode::<Time>(&crate::encode(&(24u8, 59u8, 59u8, 999_999_999u32))).is_err()
        );
        assert!(
            crate::decode::<Time>(&crate::encode(&(23u8, 60u8, 59u8, 999_999_999u32))).is_err()
        );
        assert!(
            crate::decode::<Time>(&crate::encode(&(23u8, 59u8, 60u8, 999_999_999u32))).is_err()
        );
        assert!(
            crate::decode::<Time>(&crate::encode(&(23u8, 59u8, 59u8, 1_000_000_000u32))).is_err()
        );
    }

    use alloc::vec::Vec;
    use time::Time;
    fn bench_data() -> Vec<Time> {
        crate::random_data(1000)
            .into_iter()
            .map(|(h, m, s, n): (u8, u8, u8, u32)| {
                Time::from_hms_nano(h % 24, m % 60, s % 60, n % 1_000_000_000).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(duration_vec: Vec<_>);
}
