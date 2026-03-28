use crate::{
    coder::{Decoder, View},
    derive::Decode,
    fast::{CowSlice, PushUnchecked, SliceImpl, Unaligned},
};

#[allow(unused)]
macro_rules! impl_try_convert {
    ($want: path, $have: ty) => {
        impl_try_convert!($want, $have, $have);
    };
    ($want: path, $have_encode: ty, $have_decode: ty) => {
        impl crate::derive::Encode for $want {
            type Encoder = crate::derive::convert::ConvertIntoEncoder<$have_encode>;
        }
        impl<'a> crate::derive::Decode<'a> for $want {
            type Decoder =
                crate::derive::try_convert::TryConvertFromDecoder<'a, $have_decode, $want>;
        }
    };
}

#[allow(unused)]
pub(crate) use impl_try_convert;

// Like [`TryFrom`] but we can implement it ourselves.
pub trait TryConvertFrom<T>: Sized {
    fn try_convert_from(value: T) -> Result<Self, crate::Error>;
}
/// Decodes a `T` and then converts it with [`TryConvertFrom`].
pub struct TryConvertFromDecoder<'a, T: Decode<'a>, F: TryConvertFrom<T>> {
    data: CowSlice<'a, F>,
    decoder: T::Decoder,
}

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>, F: TryConvertFrom<T>> Default for TryConvertFromDecoder<'a, T, F> {
    fn default() -> Self {
        Self {
            data: CowSlice::with_allocation(Vec::new()),
            decoder: Default::default(),
        }
    }
}

impl<'a, T: Decode<'a>, F: TryConvertFrom<T>> View<'a> for TryConvertFromDecoder<'a, T, F> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<(), crate::Error> {
        self.decoder.populate(input, length)?;

        let out: &mut Vec<F> = &mut self.data.set_owned();
        out.reserve(length);

        for _ in 0..length {
            let value = F::try_convert_from(self.decoder.decode())?;
            unsafe { out.push_unchecked(value) };
        }

        Ok(())
    }
}

impl<'a, T: Decode<'a>, F: TryConvertFrom<T> + Send + Sync> Decoder<'a, F>
    for TryConvertFromDecoder<'a, T, F>
{
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut SliceImpl<'_, Unaligned<F>>> {
        None
    }

    #[inline(always)]
    fn decode(&mut self) -> F {
        let slice = self.data.mut_slice();
        let ptr = slice.as_ptr();
        unsafe {
            let val = ptr.read();
            slice.advance(1);

            val
        }
    }
}
