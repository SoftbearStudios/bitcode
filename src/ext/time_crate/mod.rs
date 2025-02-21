mod time;

use crate::convert::impl_convert;
use crate::datetime::TimeConversion;
use crate::derive::{Decode, Encode};
use ::time::Time;

impl_convert!(Time, TimeConversion);
