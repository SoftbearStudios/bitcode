use crate::convert::{ConvertFrom, impl_convert};
use crate::derive::{Encode, Decode};
use crate::int::ranged_int;
use time::Time;

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 999_999_999);

pub type TimeConversion = (Hour, Minute, Second, Nanosecond);
impl_convert!(Time, TimeConversion);

impl ConvertFrom<&Time> for TimeConversion {
    fn convert_from(value: &Time) -> Self {
        let (hour, minute, second, nanosecond) = value.as_hms_nano();
        (
            Hour(hour),
            Minute(minute),
            Second(second),
            Nanosecond(nanosecond),
        )
    }
}

impl ConvertFrom<TimeConversion> for Time {
    fn convert_from(value: TimeConversion) -> Self {
        let (hour, minute, second, nanosecond) = value;
        hour.hint_in_range();
        minute.hint_in_range();
        second.hint_in_range();
        nanosecond.hint_in_range();
        Time::from_hms_nano(hour.0, minute.0, second.0, nanosecond.0).unwrap()
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
