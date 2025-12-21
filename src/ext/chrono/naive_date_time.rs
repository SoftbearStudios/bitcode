use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

use crate::{
    convert::{impl_convert, ConvertFrom},
    ext::chrono::{DateEncode, DateTimeDecode, DateTimeEncode, TimeEncode},
};

impl_convert!(NaiveDateTime, DateTimeEncode, DateTimeDecode);

impl ConvertFrom<&NaiveDateTime> for DateTimeEncode {
    #[inline(always)]
    fn convert_from(x: &NaiveDateTime) -> Self {
        (
            DateEncode::convert_from(&x.date()),
            TimeEncode::convert_from(&x.time()),
        )
    }
}

impl ConvertFrom<DateTimeDecode> for NaiveDateTime {
    #[inline(always)]
    fn convert_from((date, time): DateTimeDecode) -> Self {
        NaiveDateTime::new(NaiveDate::convert_from(date), NaiveTime::convert_from(time))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    use crate::decode;
    use crate::encode;

    #[test]
    fn test_chrono_naive_datetime() {
        let dt = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2025, 10, 6).unwrap(),
            NaiveTime::from_hms_nano_opt(12, 34, 56, 123_456_789).unwrap(),
        );

        let encoded = encode(&dt);
        let decoded: NaiveDateTime = decode(&encoded).unwrap();

        assert_eq!(dt, decoded);

        let dt2 = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(1, 1, 1).unwrap(),
            NaiveTime::from_hms_nano_opt(0, 0, 0, 0).unwrap(),
        );
        let encoded2 = encode(&dt2);
        let decoded2: NaiveDateTime = decode(&encoded2).unwrap();
        assert_eq!(dt2, decoded2);
    }

    fn bench_data() -> Vec<NaiveDateTime> {
        crate::random_data(1000)
            .into_iter()
            .map(
                |(y, m, d, h, min, s, n): (i32, u32, u32, u8, u8, u8, u32)| {
                    let year = (y % 9999).max(1);
                    let month = (m % 12).max(1);
                    let day = (d % 28) + 1;
                    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();

                    let hour = h % 24;
                    let minute = min % 60;
                    let second = s % 60;
                    let nano = n % 1_000_000_000;
                    let time = NaiveTime::from_hms_nano_opt(
                        hour as u32,
                        minute as u32,
                        second as u32,
                        nano,
                    )
                    .unwrap();

                    NaiveDateTime::new(date, time)
                },
            )
            .collect()
    }

    crate::bench_encode_decode!(data_vec: Vec<_>);
}
