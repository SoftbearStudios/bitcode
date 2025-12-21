use chrono::{Datelike, NaiveDate};

use crate::{
    convert::{impl_convert, ConvertFrom},
    ext::chrono::{DateDecode, DateEncode},
};

impl_convert!(NaiveDate, DateEncode, DateDecode);

impl ConvertFrom<&NaiveDate> for DateEncode {
    fn convert_from(days: &NaiveDate) -> Self {
        days.num_days_from_ce()
    }
}

impl ConvertFrom<DateDecode> for NaiveDate {
    fn convert_from(days: DateDecode) -> Self {
        NaiveDate::from_num_days_from_ce_opt(days).unwrap()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_chrono_naive_date() {
        let dates = [
            NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(), // epoch
            NaiveDate::from_ymd_opt(2025, 10, 6).unwrap(),
            NaiveDate::from_ymd_opt(1, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(-44, 3, 15).unwrap(), // BCE
            NaiveDate::from_ymd_opt(-44, 3, 15).unwrap(), // BCE
            NaiveDate::from_ymd_opt(9999, 12, 31).unwrap(),
        ];

        for x in dates {
            let enc = crate::encode(&x);
            let date: NaiveDate = crate::decode(&enc).unwrap();

            assert_eq!(x, date, "failed for date {:?}", x);
        }
    }

    use alloc::vec::Vec;
    use chrono::NaiveDate;

    fn bench_data() -> Vec<NaiveDate> {
        crate::random_data(1000)
            .into_iter()
            .map(|(y, m, d): (i32, u32, u32)| {
                let year = (y % 9999).max(1); // 1 ~ 9998
                let month = (m % 12).max(1); // 1 ~ 12
                let day = (d % 28) + 1; // 1 ~ 28
                NaiveDate::from_ymd_opt(year, month, day).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(data: Vec<_>);
}
