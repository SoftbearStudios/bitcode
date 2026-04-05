use jiff::{
    tz::{Offset, TimeZone},
    Timestamp, Zoned,
};

use crate::convert::{impl_convert, ConvertFrom};

impl_convert!(Zoned, ZonedEncoder, ZonedDecoder);

type ZonedEncoder = (Timestamp, Offset);
type ZonedDecoder = (Timestamp, Offset);

impl ConvertFrom<&Zoned> for ZonedEncoder {
    fn convert_from(value: &Zoned) -> Self {
        (value.timestamp(), value.offset())
    }
}

impl ConvertFrom<ZonedDecoder> for Zoned {
    fn convert_from(value: ZonedDecoder) -> Self {
        Zoned::new(value.0, TimeZone::fixed(value.1))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use jiff::{
        tz::{Offset, TimeZone},
        Timestamp, Zoned,
    };

    #[test]
    fn test_zoned_roundtrip() {
        let test_cases: &[Zoned] = &[
            Zoned::new(Timestamp::new(0, 0).unwrap(), TimeZone::UTC),
            Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::UTC),
            Zoned::new(Timestamp::MIN, TimeZone::UTC),
            Zoned::new(Timestamp::MAX, TimeZone::UTC),
            Zoned::new(
                Timestamp::new(123_456_789, 987_654_321).unwrap(),
                TimeZone::fixed(Offset::from_seconds(3600).unwrap()), // +01:00
            ),
            Zoned::new(
                Timestamp::new(123_456_789, 123_456_789).unwrap(),
                TimeZone::fixed(Offset::from_seconds(-7200).unwrap()), // -02:00
            ),
            Zoned::new(
                Timestamp::MIN,
                TimeZone::fixed(Offset::from_seconds(-86399).unwrap()), // -23:59:59
            ),
            Zoned::new(
                Timestamp::MAX,
                TimeZone::fixed(Offset::from_seconds(86399).unwrap()), // +23:59:59
            ),
            Zoned::new(
                Timestamp::new(-1, -1).unwrap(),
                TimeZone::fixed(Offset::from_seconds(-3600).unwrap()),
            ),
            Zoned::new(
                Timestamp::new(-100_000_000, -500_000_000).unwrap(),
                TimeZone::UTC,
            ),
        ];

        for (i, original) in test_cases.iter().enumerate() {
            let bytes = bitcode::encode(original);
            let decoded: Zoned = bitcode::decode(&bytes)
                .unwrap_or_else(|e| panic!("Failed to decode case {i}: {e}"));

            assert_eq!(
                decoded, *original,
                "Zoned roundtrip failed for case {i}:\n  original: {original}\n  decoded:  {decoded}"
            );
        }
    }

    #[test]
    fn test_zoned_decode_invalid() {
        let invalid_bytes: &[u8] = &[0xFF; 32];
        let result: Result<Zoned, _> = bitcode::decode(invalid_bytes);
        assert!(result.is_err(), "Expected decode to fail on invalid data");
    }

    fn bench_data() -> Vec<Zoned> {
        crate::random_data(1000)
            .into_iter()
            .map(|(s, n, o): (i64, i32, i32)| {
                let ts = Timestamp::new(
                    s % Timestamp::MAX.as_second(),
                    n % Timestamp::MAX.subsec_nanosecond(),
                )
                .unwrap();

                let offset_sec = o.clamp(-86399, 86399);
                let offset = Offset::from_seconds(offset_sec).unwrap_or(Offset::UTC);

                Zoned::new(ts, TimeZone::fixed(offset))
            })
            .collect()
    }

    crate::bench_encode_decode!(zoned_vec: Vec<Zoned>);
}
