use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::consume::mul_length;
use crate::derive::{Decode, Encode};
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;

#[derive(Debug)]
pub struct ArrayEncoder<T: Encode, const N: usize>(T::Encoder);

// Can't derive since it would bound T: Default.
impl<T: Encode, const N: usize> Default for ArrayEncoder<T, N> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: Encode, const N: usize> Encoder<[T; N]> for ArrayEncoder<T, N> {
    #[inline(always)]
    fn encode(&mut self, array: &[T; N]) {
        // TODO use encode_vectored if N is large enough.
        for v in array {
            self.0.encode(v);
        }
    }
}

impl<T: Encode, const N: usize> Buffer for ArrayEncoder<T, N> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(
            additional
                .checked_mul(NonZeroUsize::new(N).unwrap())
                .unwrap(),
        );
    }
}

#[derive(Debug)]
pub struct ArrayDecoder<'a, T: Decode<'a>, const N: usize>(T::Decoder);

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>, const N: usize> Default for ArrayDecoder<'a, T, N> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<'a, T: Decode<'a>, const N: usize> View<'a> for ArrayDecoder<'a, T, N> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        let length = mul_length(length, N)?;
        self.0.populate(input, length)
    }
}

impl<'a, T: Decode<'a>, const N: usize> Decoder<'a, [T; N]> for ArrayDecoder<'a, T, N> {
    #[inline(always)]
    fn decode(&mut self) -> [T; N] {
        std::array::from_fn(|_| self.0.decode())
    }

    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<[T; N]>) {
        // Safety: Equivalent to nightly MaybeUninit::transpose.
        let out = unsafe { &mut *(out.as_mut_ptr() as *mut [MaybeUninit<T>; N]) };
        for out in out {
            self.0.decode_in_place(out)
        }
    }
}
