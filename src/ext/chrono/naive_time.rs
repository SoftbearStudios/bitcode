use crate::{
    convert::{impl_convert, ConvertFrom},
    ext::chrono::{TimeDecode, TimeEncode},
};
use chrono::{NaiveTime, Timelike};

impl_convert!(NaiveTime, TimeEncode, TimeDecode);

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

impl ConvertFrom<TimeDecode> for NaiveTime {
    #[inline(always)]
    fn convert_from(value: TimeDecode) -> Self {
        let (hour, min, sec, nano) = value;

        NaiveTime::from_hms_nano_opt(
            hour.into_inner() as u32,
            min.into_inner() as u32,
            sec.into_inner() as u32,
            nano.into_inner(),
        )
        .unwrap()
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
                .is_err()
        );
    }

    use alloc::vec::Vec;
    use chrono::NaiveTime;
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
