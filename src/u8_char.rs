use crate::coder::{Buffer, Encoder};
use crate::derive::Encode;
use crate::fast::{PushUnchecked, VecImpl};
use std::num::NonZeroUsize;

/// Represents a single byte of a string, unlike u8 which represents an integer.
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct U8Char(pub u8);

// Could derive with bytemuck/derive.
unsafe impl bytemuck::Zeroable for U8Char {}
unsafe impl bytemuck::Pod for U8Char {}

impl Encode for U8Char {
    type Encoder = U8CharEncoder;
}

#[derive(Debug, Default)]
pub struct U8CharEncoder(VecImpl<U8Char>);

impl Encoder<U8Char> for U8CharEncoder {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<U8Char>> {
        Some(&mut self.0)
    }

    #[inline(always)]
    fn encode(&mut self, &v: &U8Char) {
        unsafe { self.0.push_unchecked(v) }
    }
}

impl Buffer for U8CharEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(bytemuck::must_cast_slice(self.0.as_slice()));
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}
