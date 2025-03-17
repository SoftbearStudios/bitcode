use bytemuck::CheckedBitPattern;

use crate::int::ranged_int;

use super::{
    convert::{ConvertFrom, ConvertIntoEncoder},
    Decode, Encode,
};

ranged_int!(Hour, u8, 0, 23);
ranged_int!(Minute, u8, 0, 59);
ranged_int!(Second, u8, 0, 59);
ranged_int!(Nanosecond, u32, 0, 999_999_999);

pub type TimeConversion = (Hour, Minute, Second, Nanosecond);
