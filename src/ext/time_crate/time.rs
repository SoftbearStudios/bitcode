use crate::convert::ConvertFrom;
use crate::datetime::{Hour, Minute, Nanoseconds, Second, TimeConversion};
use time::Time;

impl ConvertFrom<&Time> for TimeConversion {
    fn convert_from(value: &Time) -> Self {
        let (hour, minute, second, nanosecond) = value.as_hms_nano();
        (
            Hour(hour),
            Minute(minute),
            Second(second),
            Nanoseconds(nanosecond),
        )
    }
}

impl ConvertFrom<TimeConversion> for Time {
    fn convert_from(value: (Hour, Minute, Second, Nanoseconds)) -> Self {
        let (Hour(hour), Minute(minute), Second(second), Nanoseconds(nanosecond)) = value;
        // Safety: should not fail because all input values are validated with CheckedBitPattern.
        unsafe { Time::from_hms_nano(hour, minute, second, nanosecond).unwrap_unchecked() }
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
