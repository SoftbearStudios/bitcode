use super::convert::{impl_convert, ConvertFrom};
use crate::int::ranged_int;
use core::time::Duration;

ranged_int!(Nanosecond, u32, 0, 999_999_999);

type DurationEncode = (u64, u32);
type DurationDecode = (u64, Nanosecond);

impl ConvertFrom<&Duration> for DurationEncode {
    #[inline(always)]
    fn convert_from(value: &Duration) -> Self {
        (value.as_secs(), value.subsec_nanos())
    }
}

impl ConvertFrom<DurationDecode> for Duration {
    #[inline(always)]
    fn convert_from(value: DurationDecode) -> Self {
        Duration::new(value.0, value.1.into_inner())
    }
}

impl_convert!(Duration, DurationEncode, DurationDecode);

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        assert!(crate::decode::<Duration>(&crate::encode(&(u64::MAX, 999_999_999))).is_ok());
        assert!(crate::decode::<Duration>(&crate::encode(&(u64::MAX, 1_000_000_000))).is_err());
    }

    use alloc::vec::Vec;
    use core::time::Duration;
    fn bench_data() -> Vec<Duration> {
        crate::random_data(1000)
            .into_iter()
            .map(|(s, n): (_, u32)| Duration::new(s, n % 1_000_000_000))
            .collect()
    }
    crate::bench_encode_decode!(duration_vec: Vec<Duration>);
}
