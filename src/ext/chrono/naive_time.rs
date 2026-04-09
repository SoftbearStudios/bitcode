use crate::{
    convert::ConvertFrom,
    int::ranged_int,
    try_convert::{impl_try_convert, TryConvertFrom},
};
use chrono::{NaiveTime, Timelike};

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 1_999_999_999);

type TimeEncode = (u8, u8, u8, u32);
type TimeDecode = (Hour, Minute, Second, Nanosecond);

impl_try_convert!(NaiveTime, TimeEncode, TimeDecode);

impl ConvertFrom<&NaiveTime> for TimeEncode {
    #[inline(always)]
    fn convert_from(value: &NaiveTime) -> Self {
        (
            value.hour() as u8,
            value.minute() as u8,
            value.second() as u8,
            value.nanosecond(),
        )
    }
}

impl TryConvertFrom<TimeDecode> for NaiveTime {
    #[inline(always)]
    fn try_convert_from(value: TimeDecode) -> Result<Self, crate::Error> {
        let (hour, min, sec, nano) = value;

        NaiveTime::from_hms_nano_opt(
            hour.into_inner() as u32,
            min.into_inner() as u32,
            sec.into_inner() as u32,
            nano.into_inner(),
        )
        .ok_or_else(|| crate::error::error("Failed to convert TimeDecode to NaiveTime"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_chrono_naive_time() {
        assert!(crate::decode::<NaiveTime>(&crate::encode(
            &NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap()
        ))
        .is_ok());
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(23u8, 59u8, 59u8, 999_999_999u32))).is_ok()
        );
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(24u8, 59u8, 59u8, 999_999_999u32)))
                .is_err()
        );
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(23u8, 60u8, 59u8, 999_999_999u32)))
                .is_err()
        );
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(23u8, 59u8, 60u8, 999_999_999u32)))
                .is_err()
        );
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(23u8, 59u8, 59u8, 1_000_000_000u32)))
                .is_ok()
        );
        assert!(
            crate::decode::<NaiveTime>(&crate::encode(&(23u8, 59u8, 58u8, 1_000_000_000u32)))
                .is_err()
        );
    }

    use alloc::vec::Vec;
    use chrono::NaiveTime;
    fn bench_data() -> Vec<NaiveTime> {
        crate::random_data(1000)
            .into_iter()
            .map(|(h, m, s, n): (u32, u32, u32, u32)| {
                NaiveTime::from_hms_nano_opt(h % 24, m % 60, s % 60, n % 1_000_000_000).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(duration_vec: Vec<_>);
}
