use jiff::{
    tz::{Offset, TimeZone},
    Timestamp, Zoned,
};

use crate::{
    convert::ConvertFrom,
    ext::jiff::{
        offset::{OffsetDecoder, OffsetEncoder},
        timestamp::{TimestampDecoder, TimestampEncode},
    },
    try_convert::{impl_try_convert, TryConvertFrom},
};

impl_try_convert!(Zoned, ZonedEncoder, ZonedDecoder);

type ZonedEncoder = (TimestampEncode, OffsetEncoder);
type ZonedDecoder = (TimestampDecoder, OffsetDecoder);

impl ConvertFrom<&Zoned> for ZonedEncoder {
    fn convert_from(value: &Zoned) -> Self {
        (
            TimestampEncode::convert_from(&value.timestamp()),
            OffsetEncoder::convert_from(&value.offset()),
        )
    }
}
impl TryConvertFrom<ZonedDecoder> for Zoned {
    fn try_convert_from(value: ZonedDecoder) -> Result<Self, crate::Error> {
        let timestamp = Timestamp::try_convert_from(value.0)?;
        let offset = Offset::try_convert_from(value.1)?;

        Ok(Zoned::new(timestamp, TimeZone::fixed(offset)))
    }
}
