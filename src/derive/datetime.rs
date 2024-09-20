use bytemuck::CheckedBitPattern;

use super::Decode;

/// A u8 guaranteed to be < 24.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Hour(pub u8);
// Safety: u8 and Hour have the same layout since Hour is #[repr(transparent)].
unsafe impl CheckedBitPattern for Hour {
    type Bits = u8;
    #[inline(always)]
    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        *bits < 24
    }
}
impl<'a> Decode<'a> for Hour {
    type Decoder = crate::int::CheckedIntDecoder<'a, Hour, u8>;
}

/// A u8 guaranteed to be < 60.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Minute(pub u8);
// Safety: u8 and Minute have the same layout since Minute is #[repr(transparent)].
unsafe impl CheckedBitPattern for Minute {
    type Bits = u8;
    #[inline(always)]
    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        *bits < 60
    }
}
impl<'a> Decode<'a> for Minute {
    type Decoder = crate::int::CheckedIntDecoder<'a, Minute, u8>;
}

/// A u8 guaranteed to be < 60.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Second(pub u8);
// Safety: u8 and Second have the same layout since Second is #[repr(transparent)].
unsafe impl CheckedBitPattern for Second {
    type Bits = u8;
    #[inline(always)]
    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        *bits < 60
    }
}
impl<'a> Decode<'a> for Second {
    type Decoder = crate::int::CheckedIntDecoder<'a, Second, u8>;
}

/// A u32 guaranteed to be < 1 billion.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Nanoseconds(pub u32);
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
