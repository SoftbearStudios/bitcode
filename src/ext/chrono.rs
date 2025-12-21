mod date_time_utc;
mod naive_date;
mod naive_date_time;
mod naive_time;

use crate::int::ranged_int;

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 1_999_999_999);

type TimeEncode = (u8, u8, u8, u32);
type TimeDecode = (Hour, Minute, Second, Nanosecond);

type DateEncode = i32;
type DateDecode = i32;

type DateTimeEncode = (DateEncode, TimeEncode);
type DateTimeDecode = (DateEncode, TimeDecode);
