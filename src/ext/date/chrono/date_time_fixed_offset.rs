use chrono::{DateTime, FixedOffset, NaiveDateTime};

use crate::{
    convert::{impl_convert, ConvertFrom},
    ext::date::{DateTimeEncode, DateTimeWithOffsetDecode, DateTimeWithOffsetEncode},
};

impl_convert!(
    DateTime<FixedOffset>,
    DateTimeWithOffsetEncode,
    DateTimeWithOffsetDecode
);

impl ConvertFrom<&DateTime<FixedOffset>> for DateTimeWithOffsetEncode {
    fn convert_from(x: &DateTime<FixedOffset>) -> Self {
        let naive_enc = DateTimeEncode::convert_from(&x.naive_utc());
        let offset_sec = x.offset().local_minus_utc();

        (naive_enc, offset_sec)
    }
}

impl ConvertFrom<DateTimeWithOffsetEncode> for DateTime<FixedOffset> {
    fn convert_from(enc: DateTimeWithOffsetEncode) -> Self {
        let naive = NaiveDateTime::convert_from(enc.0);
        let offset =
            FixedOffset::east_opt(enc.1).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());

        DateTime::<FixedOffset>::from_naive_utc_and_offset(naive, offset)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use chrono::{DateTime, FixedOffset, NaiveDate};

    #[test]
    fn test_chrono_datetime_fixedoffset() {
        let dates = [
            (1, 1, 1),
            (1970, 1, 1), // epoch
            (2025, 10, 6),
            (-44, 3, 15), // BCE
            (9999, 12, 31),
        ];

        let offsets = [
            -12 * 3600,      // UTC-12, Baker Island Time
            -11 * 3600,      // UTC-11, Niue / Samoa
            -5 * 3600,       // UTC-5, EST (Eastern Standard Time, 美东冬令时)
            -3 * 3600,       // UTC-3, BRT (Brasilia Time)
            0,               // UTC+0, GMT
            3600,            // UTC+1, CET (Central European Time)
            3 * 3600,        // UTC+3, MSK (Moscow Time)
            5 * 3600 + 1800, // UTC+5:30, IST (India Standard Time)
            8 * 3600,        // UTC+8, CST (China Standard Time)
            14 * 3600,       // UTC+14, Line Islands Time
        ];

        let times = [(0, 0, 0), (12, 34, 56), (23, 59, 59)];

        for &(y, m, d) in &dates {
            for &(h, mi, s) in &times {
                let naive = NaiveDate::from_ymd_opt(y, m, d)
                    .unwrap()
                    .and_hms_opt(h, mi, s)
                    .unwrap();

                for &offset_sec in &offsets {
                    let offset = FixedOffset::east_opt(offset_sec).unwrap();
                    let dt_fixed =
                        DateTime::<FixedOffset>::from_naive_utc_and_offset(naive, offset);

                    let enc = crate::encode(&dt_fixed);
                    let decoded: DateTime<FixedOffset> = crate::decode(&enc).unwrap();

                    assert_eq!(
                        dt_fixed, decoded,
                        "Failed for datetime {:?} with offset {}",
                        dt_fixed, offset
                    );
                }
            }
        }
    }

    fn bench_data() -> Vec<DateTime<FixedOffset>> {
        crate::random_data(1000)
            .into_iter()
            .map(
                |(y, m, d, h, mi, s, n, offset_sec): (i32, u32, u32, u32, u32, u32, u32, i32)| {
                    let naive =
                        NaiveDate::from_ymd_opt((y % 9999).max(1), (m % 12).max(1), (d % 28) + 1)
                            .unwrap()
                            .and_hms_nano_opt(h % 24, mi % 60, s % 60, n % 1_000_000_000)
                            .unwrap();
                    let offset = FixedOffset::east_opt(offset_sec % 86_400)
                        .unwrap_or(FixedOffset::east_opt(0).unwrap());
                    DateTime::<FixedOffset>::from_naive_utc_and_offset(naive, offset)
                },
            )
            .collect()
    }

    crate::bench_encode_decode!(data: Vec<DateTime<FixedOffset>>);
}
