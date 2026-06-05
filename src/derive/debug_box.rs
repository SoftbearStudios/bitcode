//! [`Box`] indirection for derived encoders/decoders, used only in debug mode
//! to avoid stack overflow.

use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::fast::{SliceImpl, Unaligned, VecImpl};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;

/// Wraps a derived [`Encoder`] in a [`Box`] and delegates to it.
pub struct BoxEncoder<E>(Box<E>);

impl<E: Default> Default for BoxEncoder<E> {
    #[inline(never)]
    fn default() -> Self {
        Self(Box::default())
    }
}

impl<E: Buffer> Buffer for BoxEncoder<E> {
    #[inline(never)]
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }

    #[inline(never)]
    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

impl<T: ?Sized, E: Encoder<T>> Encoder<T> for BoxEncoder<E> {
    #[inline(never)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<T>>
    where
        T: Sized,
    {
        self.0.as_primitive()
    }

    #[inline(never)]
    fn encode(&mut self, t: &T) {
        self.0.encode(t);
    }

    #[inline(never)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a T> + Clone)
    where
        T: 'a,
    {
        self.0.encode_vectored(i);
    }
}

/// Wraps a derived [`Decoder`] in a [`Box`] and delegates to it.
pub struct BoxDecoder<D>(Box<D>);

impl<D: Default> Default for BoxDecoder<D> {
    #[inline(never)]
    fn default() -> Self {
        Self(Box::default())
    }
}

impl<'a, D: View<'a>> View<'a> for BoxDecoder<D> {
    #[inline(never)]
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)
    }
}

impl<'a, T, D: Decoder<'a, T>> Decoder<'a, T> for BoxDecoder<D> {
    #[inline(never)]
    fn as_primitive(&mut self) -> Option<&mut SliceImpl<'_, Unaligned<T>>> {
        self.0.as_primitive()
    }

    #[inline(never)]
    fn decode(&mut self) -> T {
        self.0.decode()
    }

    #[inline(never)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<T>) {
        self.0.decode_in_place(out);
    }
}
