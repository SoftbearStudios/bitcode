use crate::int::ranged_int;
#[cfg(feature = "chrono")]
mod chrono;
#[cfg(feature = "time")]
mod time;

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 999_999_999);

type TimeEncode = (u8, u8, u8, u32);
type TimeDecode = (Hour, Minute, Second, Nanosecond);

#[cfg(feature = "chrono")]
type DateEncode = i32;
#[cfg(feature = "chrono")]
type DateDecode = i32;

#[cfg(feature = "chrono")]
type DateTimeEncode = (DateEncode, TimeEncode);
#[cfg(feature = "chrono")]
type DateTimeDecode = (DateEncode, TimeEncode);

#[cfg(feature = "chrono")]
pub type DateTimeWithOffsetEncode = (DateTimeEncode, i32);
#[cfg(feature = "chrono")]
pub type DateTimeWithOffsetDecode = (DateTimeDecode, i32);
