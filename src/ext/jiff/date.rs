use jiff::civil::Date;

use crate::{
    convert::ConvertFrom,
    error::error,
    int::ranged_int,
    try_convert::{impl_try_convert, TryConvertFrom},
};

impl_try_convert!(Date, DateEncode, DateDecode);

// The value is guaranteed to be in the range `-9999..=9999`.
ranged_int!(Year, i16, -9999, 9999);
// The value is guaranteed to be in the range `1..=12`.
ranged_int!(Month, u8, 1, 12);
// The value is guaranteed to be in the range `0..=59`.
ranged_int!(Day, u8, 1, 31);

pub type DateEncode = (i16, u8, u8);
pub type DateDecode = (Year, Month, Day);

impl ConvertFrom<&Date> for DateEncode {
    fn convert_from(value: &Date) -> Self {
        (value.year(), value.month() as u8, value.day() as u8)
    }
}

impl TryConvertFrom<DateDecode> for Date {
    fn try_convert_from(value: DateDecode) -> Result<Self, crate::Error> {
        Date::new(
            value.0.into_inner(),
            value.1.into_inner() as i8,
            value.2.into_inner() as i8,
        )
        .map_err(|_| error("Failed to decode date"))
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::Date;

    #[test]
    fn test_date() {
        // -9999-01-01
        let date = Date::new(-9999, 1, 1).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        // 9999-12-30
        let date = Date::new(9999, 12, 30).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        // 2025-03-28
        let date = Date::new(2025, 3, 28).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        let date = Date::new(2025, 1, 15).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        let date = Date::new(2025, 12, 15).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        let date = Date::new(2025, 4, 30).unwrap();
        let bytes = crate::encode(&date);
        let decoded: Date = crate::decode(&bytes).unwrap();
        assert_eq!(decoded, date);

        let bytes = crate::encode(&(-10000i16, 1u8, 1u8));
        let result: Result<Date, _> = crate::decode(&bytes);
        assert!(result.is_err());

        let bytes = crate::encode(&(10000i16, 1u8, 1u8));
        let result: Result<Date, _> = crate::decode(&bytes);
        assert!(result.is_err());

        let bytes = crate::encode(&(2025i16, 0u8, 1u8));
        let result: Result<Date, _> = crate::decode(&bytes);
        assert!(result.is_err());

        let bytes = crate::encode(&(2025i16, 13u8, 1u8));
        let result: Result<Date, _> = crate::decode(&bytes);
        assert!(result.is_err());

        let bytes = crate::encode(&(2025i16, 1u8, 0u8));
        let result: Result<Date, _> = crate::decode(&bytes);
        assert!(result.is_err());

        let date = Date::new(2025, 3, 28).unwrap();
        assert!(crate::decode::<Date>(&crate::encode(&date)).is_ok());
    }

    use alloc::vec::Vec;

    fn bench_data() -> Vec<Date> {
        crate::random_data(1000)
            .into_iter()
            .map(|(year, month, day): (i16, i8, i8)| {
                let year = year.clamp(-9999, 9999);
                let month = month.clamp(1, 12);
                let day = day.clamp(1, 28);

                Date::new(year, month, day).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(date_vec: Vec<_>);
}
