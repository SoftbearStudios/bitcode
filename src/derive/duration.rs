use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::{Decode, Encode};
use alloc::vec::Vec;
use bytemuck::CheckedBitPattern;
use core::num::NonZeroUsize;
use core::time::Duration;

#[derive(Default)]
pub struct DurationEncoder {
    secs: <u64 as Encode>::Encoder,
    subsec_nanos: <u32 as Encode>::Encoder,
}
impl Encoder<Duration> for DurationEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &Duration) {
        self.secs.encode(&t.as_secs());
        self.subsec_nanos.encode(&t.subsec_nanos());
    }
}
impl Buffer for DurationEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.secs.collect_into(out);
        self.subsec_nanos.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.secs.reserve(additional);
        self.subsec_nanos.reserve(additional);
    }
}
impl Encode for Duration {
    type Encoder = DurationEncoder;
}

/// A u32 guaranteed to be < 1 billion. Prevents Duration::new from panicking.
#[derive(Copy, Clone)]
#[repr(transparent)]
struct Nanoseconds(u32);
// Safety: u32 and Nanoseconds have the same layout since Nanoseconds is #[repr(transparent)].
unsafe impl CheckedBitPattern for Nanoseconds {
    type Bits = u32;
    #[inline(always)]
    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        *bits < 1_000_000_000
    }
}
impl<'a> Decode<'a> for Nanoseconds {
    type Decoder = crate::int::CheckedIntDecoder<'a, Nanoseconds, u32>;
}

#[derive(Default)]
pub struct DurationDecoder<'a> {
    secs: <u64 as Decode<'a>>::Decoder,
    subsec_nanos: <Nanoseconds as Decode<'a>>::Decoder,
}
impl<'a> View<'a> for DurationDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.secs.populate(input, length)?;
        self.subsec_nanos.populate(input, length)?;
        Ok(())
    }
}
impl<'a> Decoder<'a, Duration> for DurationDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> Duration {
        let secs = self.secs.decode();
        let Nanoseconds(subsec_nanos) = self.subsec_nanos.decode();
        // Makes Duration::new 4x faster since it can skip checks and division.
        // Safety: impl CheckedBitPattern for Nanoseconds guarantees this.
        unsafe {
            if !Nanoseconds::is_valid_bit_pattern(&subsec_nanos) {
                core::hint::unreachable_unchecked();
            }
        }
        Duration::new(secs, subsec_nanos)
    }
}
impl<'a> Decode<'a> for Duration {
    type Decoder = DurationDecoder<'a>;
}

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
    crate::bench_encode_decode!(duration_vec: Vec<_>);
}
