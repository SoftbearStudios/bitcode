use chrono::{DateTime, NaiveDateTime, Utc};

use crate::{
    convert::{impl_convert, ConvertFrom},
    ext::chrono::{DateTimeDecode, DateTimeEncode},
};

impl_convert!(DateTime<Utc>, DateTimeEncode, DateTimeDecode);

impl ConvertFrom<&DateTime<Utc>> for DateTimeEncode {
    fn convert_from(x: &DateTime<Utc>) -> Self {
        DateTimeEncode::convert_from(&x.naive_utc())
    }
}

impl ConvertFrom<DateTimeDecode> for DateTime<Utc> {
    fn convert_from(enc: DateTimeDecode) -> Self {
        let naive = NaiveDateTime::convert_from(enc);

        DateTime::from_naive_utc_and_offset(naive, Utc)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use chrono::{DateTime, NaiveDate, Utc};

    #[test]
    fn test_chrono_datetime_utc() {
        let ymds = [
            (1970, 1, 1), // epoch
            (2025, 10, 6),
            (1, 1, 1),
            (-44, 3, 15), // BCE
            (9999, 12, 31),
        ];

        for &(y, m, d) in ymds.iter() {
            let naive = NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(12, 34, 56)
                .unwrap();
            let dt_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);

            let enc = crate::encode(&dt_utc);
            let decoded: DateTime<Utc> = crate::decode(&enc).unwrap();

            assert_eq!(dt_utc, decoded, "failed for datetime {:?}", dt_utc);
        }
    }

    fn bench_data() -> Vec<DateTime<Utc>> {
        crate::random_data(1000)
            .into_iter()
            .map(
                |(y, m, d, h, mi, s, n, _offset_sec): (i32, u32, u32, u32, u32, u32, u32, i32)| {
                    let naive =
                        NaiveDate::from_ymd_opt((y % 9999).max(1), (m % 12).max(1), (d % 28) + 1)
                            .unwrap()
                            .and_hms_nano_opt(h % 24, mi % 60, s % 60, n % 1_000_000_000)
                            .unwrap();
                    DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
                },
            )
            .collect()
    }

    crate::bench_encode_decode!(utc_vec: Vec<DateTime<Utc>>);
}
