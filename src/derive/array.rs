use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::consume::mul_length;
use crate::derive::{Decode, Encode};
use crate::fast::{FastSlice, FastVec, Unaligned};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;

pub struct ArrayEncoder<T: Encode, const N: usize>(T::Encoder);

// Can't derive since it would bound T: Default.
impl<T: Encode, const N: usize> Default for ArrayEncoder<T, N> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: Encode, const N: usize> Encoder<[T; N]> for ArrayEncoder<T, N> {
    fn as_primitive(&mut self) -> Option<&mut FastVec<[T; N]>> {
        // FastVec doesn't work on ZST.
        if N == 0 {
            return None;
        }
        self.0.as_primitive().map(|v| {
            debug_assert!(v.len() % N == 0);
            // Safety: FastVec uses pointers for len/cap unlike Vec, so casting to FastVec<[T; N]>
            // is safe as long as `v.len() % N == 0`. This will always be the case since we only
            // encode in chunks of N.
            // NOTE: If panics occurs during ArrayEncoder::encode and Buffer is reused, this
            // invariant can be violated. Luckily primitive encoders never panic.
            // TODO std::mem::take Buffer while encoding to avoid corrupted buffers.
            unsafe { core::mem::transmute(v) }
        })
    }

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
        if N == 0 {
            return; // self.0.reserve takes NonZeroUsize and `additional * N == 0`.
        }
        self.0.reserve(
            additional
                .checked_mul(NonZeroUsize::new(N).unwrap())
                .unwrap(),
        );
    }
}

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
    fn as_primitive(&mut self) -> Option<&mut FastSlice<'_, Unaligned<[T; N]>>> {
        self.0.as_primitive().map(|s| {
            // Safety: FastSlice doesn't have a length unlike slice, so casting to FastSlice<[T; N]>
            // is safe. N == 0 case is also safe for the same reason.
            unsafe { core::mem::transmute(s) }
        })
    }

    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<[T; N]>) {
        // Safety: Equivalent to nightly MaybeUninit::transpose.
        let out = unsafe { &mut *(out.as_mut_ptr() as *mut [MaybeUninit<T>; N]) };
        for out in out {
            self.0.decode_in_place(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::coder::{Buffer, Encoder};
    use crate::error::err;
    use crate::length::LengthEncoder;
    use crate::{decode, encode};
    use alloc::vec::Vec;
    use core::num::NonZeroUsize;

    #[test]
    fn test_empty_array() {
        type T = [u8; 0];
        let empty_array = T::default();
        decode::<T>(&encode(&empty_array)).unwrap();
        decode::<Vec<T>>(&encode(&vec![empty_array; 100])).unwrap();
    }

    #[test]
    fn test_length_overflow() {
        const N: usize = 16384;
        let mut encoder = LengthEncoder::default();
        encoder.reserve(NonZeroUsize::MIN);
        encoder.encode(&(usize::MAX / N + 1));
        let bytes = encoder.collect();
        assert_eq!(decode::<Vec<[u8; N]>>(&bytes), err("length overflow"));
    }

    fn bench_data() -> Vec<Vec<[u8; 3]>> {
        crate::random_data::<u8>(125)
            .into_iter()
            .map(|n| (0..n / 16).map(|_| [0, 0, 255]).collect())
            .collect()
    }
    crate::bench_encode_decode!(u8_array_vecs: Vec<Vec<[u8; 3]>>);
}
