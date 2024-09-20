use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::datetime::{Hour, Minute, Nanoseconds, Second};
use crate::{Decode, Encode};
use alloc::vec::Vec;
use core::num::NonZeroUsize;
use time::Time;

#[derive(Default)]
pub struct TimeEncoder {
    hour: <u8 as Encode>::Encoder,
    minute: <u8 as Encode>::Encoder,
    second: <u8 as Encode>::Encoder,
    nanosecond: <u32 as Encode>::Encoder,
}
impl Encoder<Time> for TimeEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &Time) {
        let (hour, minute, second, nanosecond) = t.as_hms_nano();
        self.hour.encode(&hour);
        self.minute.encode(&minute);
        self.second.encode(&second);
        self.nanosecond.encode(&nanosecond);
    }
}
impl Buffer for TimeEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.hour.collect_into(out);
        self.minute.collect_into(out);
        self.second.collect_into(out);
        self.nanosecond.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.hour.reserve(additional);
        self.minute.reserve(additional);
        self.second.reserve(additional);
        self.nanosecond.reserve(additional);
    }
}
impl Encode for Time {
    type Encoder = TimeEncoder;
}

#[derive(Default)]
pub struct TimeDecoder<'a> {
    hour: <Hour as Decode<'a>>::Decoder,
    minute: <Minute as Decode<'a>>::Decoder,
    second: <Second as Decode<'a>>::Decoder,
    nanosecond: <Nanoseconds as Decode<'a>>::Decoder,
}
impl<'a> View<'a> for TimeDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.hour.populate(input, length)?;
        self.minute.populate(input, length)?;
        self.second.populate(input, length)?;
        self.nanosecond.populate(input, length)?;
        Ok(())
    }
}
impl<'a> Decoder<'a, Time> for TimeDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> Time {
        let Hour(hour) = self.hour.decode();
        let Minute(minute) = self.minute.decode();
        let Second(second) = self.second.decode();
        let Nanoseconds(nanosecond) = self.nanosecond.decode();
        // Safety: should not fail because all input values are validated with CheckedBitPattern.
        unsafe { Time::from_hms_nano(hour, minute, second, nanosecond).unwrap_unchecked() }
    }
}
impl<'a> Decode<'a> for Time {
    type Decoder = TimeDecoder<'a>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        assert!(crate::decode::<Time>(&crate::encode(&(23, 59, 59, 999_999_999))).is_ok());
        assert!(crate::decode::<Time>(&crate::encode(&(24, 59, 59, 999_999_999))).is_err());
        assert!(crate::decode::<Time>(&crate::encode(&(23, 60, 59, 999_999_999))).is_err());
        assert!(crate::decode::<Time>(&crate::encode(&(23, 59, 60, 999_999_999))).is_err());
        assert!(crate::decode::<Time>(&crate::encode(&(23, 59, 59, 1_000_000_000))).is_err());
    }

    use alloc::vec::Vec;
    use time::Time;
    fn bench_data() -> Vec<Time> {
        crate::random_data(1000)
            .into_iter()
            .map(|(h, m, s, n): (u8, u8, u8, u32)| {
                Time::from_hms_nano(h % 24, m % 60, s % 60, n % 1_000_000_000).unwrap()
            })
            .collect()
    }
    crate::bench_encode_decode!(duration_vec: Vec<_>);
}
