use jiff::civil::Time;

use crate::{
    convert::{impl_convert, ConvertFrom},
    int::ranged_int,
};

impl_convert!(Time, TimeEncode, TimeDecode);

// The value is guaranteed to be in the range `0..=23`.
ranged_int!(Hour, u8, 0, 23);
// The value is guaranteed to be in the range `0..=59`.
ranged_int!(Minute, u8, 0, 59);
// The value is guaranteed to be in the range `0..=59`.
ranged_int!(Second, u8, 0, 59);
// The value is guaranteed to be in the range `0..=999_999_999`
ranged_int!(Nanosecond, u32, 0, 999_999_999);

type TimeEncode = (u8, u8, u8, u32);
type TimeDecode = (Hour, Minute, Second, Nanosecond);

impl ConvertFrom<&Time> for TimeEncode {
    fn convert_from(value: &Time) -> Self {
        (
            value.hour() as u8,
            value.minute() as u8,
            value.second() as u8,
            value.subsec_nanosecond() as u32,
        )
    }
}

impl ConvertFrom<TimeDecode> for Time {
    fn convert_from(value: TimeDecode) -> Self {
        Time::constant(
            value.0.into_inner() as i8,
            value.1.into_inner() as i8,
            value.2.into_inner() as i8,
            value.3.into_inner() as i32,
        )
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn test_time() {
        // 00:00:00.000000000
        let time = Time::constant(0, 0, 0, 0);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        // 23:59:59.999999999
        let time = Time::constant(23, 59, 59, 999_999_999);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        // 23:00:00
        let time = Time::constant(23, 0, 0, 0);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        // 00:59:00
        let time = Time::constant(0, 59, 0, 0);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        // 00:00:59
        let time = Time::constant(0, 0, 59, 0);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        // 12:30:45.123456789
        let time = Time::constant(12, 30, 45, 123456789);
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        let time = Time::default();
        let bytes = bitcode::encode(&time);
        let decoded: Time = bitcode::decode(&bytes).unwrap();
        assert_eq!(decoded, time);

        assert!(crate::decode::<Time>(&crate::encode(&Time::default())).is_ok());
    }

    use alloc::vec::Vec;
    use jiff::civil::Time;

    fn bench_data() -> Vec<Time> {
        crate::random_data(1000)
            .into_iter()
            .map(|(h, m, s, n): (u8, u8, u8, u32)| {
                Time::constant(
                    (h % 23) as i8,
                    (m % 59) as i8,
                    (s % 59) as i8,
                    (n % 999_999_999) as i32,
                )
            })
            .collect()
    }
    crate::bench_encode_decode!(time_vec: Vec<_>);
}
