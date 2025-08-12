use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use core::num::NonZeroUsize;

#[allow(unused)]
macro_rules! impl_convert {
    ($want: path, $have: ty) => {
        impl Encode for $want {
            type Encoder = crate::derive::convert::ConvertIntoEncoder<$have>;
        }
        impl<'a> Decode<'a> for $want {
            type Decoder = crate::derive::convert::ConvertFromDecoder<'a, $have>;
        }
    };
}

#[allow(unused)]
pub(crate) use impl_convert;

// Like [`From`] but we can implement it ourselves.
pub(crate) trait ConvertFrom<T>: Sized {
    fn convert_from(value: T) -> Self;
}

pub struct ConvertIntoEncoder<T: Encode>(T::Encoder);

// Can't derive since it would bound T: Default.
impl<T: Encode> Default for ConvertIntoEncoder<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<D, T: Encode + for<'a> ConvertFrom<&'a D>> Encoder<D> for ConvertIntoEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &D) {
        self.0.encode(&T::convert_from(t));
    }
}

impl<T: Encode> Buffer for ConvertIntoEncoder<T> {
    fn collect_into(&mut self, out: &mut alloc::vec::Vec<u8>) {
        self.0.collect_into(out);
    }
    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

/// Decodes a `T` and then converts it with [`ConvertFrom`].
pub struct ConvertFromDecoder<'a, T: Decode<'a>>(T::Decoder);

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>> Default for ConvertFromDecoder<'a, T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<'a, T: Decode<'a>> View<'a> for ConvertFromDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)
    }
}

impl<'a, F: ConvertFrom<T>, T: Decode<'a>> Decoder<'a, F> for ConvertFromDecoder<'a, T> {
    #[inline(always)]
    fn decode(&mut self) -> F {
        F::convert_from(self.0.decode())
    }
}
