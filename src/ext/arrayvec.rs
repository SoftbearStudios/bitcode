use crate::coder::{Decoder, Encoder, Result, View};
use crate::derive::vec::{unsafe_wild_copy, VecDecoder, VecEncoder};
use crate::derive::{Decode, Encode};
use crate::error::err;
use crate::str::{StrDecoder, StrEncoder};
use arrayvec::{ArrayString, ArrayVec};
use std::mem::MaybeUninit;

// TODO optimize ArrayVec impls and make ArrayString use them.
impl<const N: usize> Encoder<ArrayString<N>> for StrEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &ArrayString<N>) {
        // Only lengths < 255 are fast to encode and avoid copying lots of memory for 1 byte strings.
        // TODO miri doesn't like ArrayString::as_str().as_ptr(), replace with ArrayString::as_ptr() when available.
        if N > 64 || cfg!(miri) {
            self.encode(t.as_str());
            return;
        }

        let s = t.as_str();
        self.0.lengths.encode_less_than_255(s.len());
        let primitives = self.0.elements.as_primitive().unwrap();
        primitives.reserve(N); // TODO Buffer::reserve impl additional * N so we can remove encode_vectored impl.
        let dst = primitives.end_ptr();

        // Safety: `s.as_ptr()` points to `N` valid bytes since it's referencing an ArrayString<N>.
        // `dst` has enough space for `[T; N]` because we've reserved `N`.
        unsafe {
            *(dst as *mut MaybeUninit<[u8; N]>) = *(s.as_ptr() as *const MaybeUninit<[u8; N]>);
            primitives.set_end_ptr(dst.add(s.len()));
        }
    }
    #[inline(never)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a ArrayString<N>> + Clone) {
        // Only lengths < 255 are fast to encode and avoid copying lots of memory for 1 byte strings.
        // TODO miri doesn't like ArrayString::as_str().as_ptr(), replace with ArrayString::as_ptr() when available.
        if N > 64 || cfg!(miri) {
            self.encode_vectored(i.map(|t| t.as_str()));
            return;
        }

        // This encode_vectored impl is same as encode impl, but pulls the reserve out of the loop.
        let primitives = self.0.elements.as_primitive().unwrap();
        primitives.reserve(i.size_hint().1.unwrap() * N);
        let mut dst = primitives.end_ptr();
        for t in i {
            let s = t.as_str();
            self.0.lengths.encode_less_than_255(s.len());
            // Safety: `s.as_ptr()` points to `N` valid bytes since it's referencing an ArrayString<N>.
            // `dst` has enough space for `[T; N]` because we've reserved `size_hint * N`.
            unsafe {
                *(dst as *mut MaybeUninit<[u8; N]>) = *(s.as_ptr() as *const MaybeUninit<[u8; N]>);
                dst = dst.add(s.len());
            }
        }
        primitives.set_end_ptr(dst);
    }
}
impl<const N: usize> Encode for ArrayString<N> {
    type Encoder = StrEncoder;
}

// TODO replace with StrDecoder<N> that optimizes calls to LengthDecoder<N>::decode.
#[derive(Default)]
pub struct ArrayStringDecoder<'a, const N: usize>(StrDecoder<'a>);
impl<'a, const N: usize> View<'a> for ArrayStringDecoder<'a, N> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)?;
        // Safety: `length` was same length passed to populate.
        if unsafe { self.0.lengths.any_greater_than::<N>(length) } {
            return err("invalid ArrayString");
        }
        Ok(())
    }
}
impl<'a, const N: usize> Decoder<'a, ArrayString<N>> for ArrayStringDecoder<'a, N> {
    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<ArrayString<N>>) {
        let s: &str = self.0.decode();
        let array_string = out.write(ArrayString::new());

        // Avoid copying lots of memory for 1 byte strings.
        // TODO miri doesn't like ArrayString::as_mut_str().as_mut_ptr(), replace with ArrayString::as_mut_ptr() when available.
        if N > 64 || cfg!(miri) {
            // Safety: We've ensured `self.lengths.max_len() <= N` in populate.
            unsafe { array_string.try_push_str(s).unwrap_unchecked() };
            return;
        }
        // Empty s points to no valid bytes, so we can't unsafe_wild_copy.
        if s.is_empty() {
            return;
        }
        // Safety: We just checked n != 0 and ensured `self.lengths.max_len() <= N` in populate.
        // Also, `dst` has room for `[u8; N]` since it's an ArrayString<N>.
        unsafe {
            let src = s.as_ptr();
            let dst = array_string.as_mut_str().as_mut_ptr();
            let n = s.len();
            unsafe_wild_copy!([u8; N], src, dst, n);
            array_string.set_len(s.len());
        }
    }
}
impl<'a, const N: usize> Decode<'a> for ArrayString<N> {
    type Decoder = ArrayStringDecoder<'a, N>;
}

// Helps optimize out some checks in `LengthEncoder::encode`.
#[inline(always)]
fn as_slice_assert_len<T, const N: usize>(t: &ArrayVec<T, N>) -> &[T] {
    let s = t.as_slice();
    // Safety: ArrayVec<N> has length <= N. TODO replace with LengthDecoder<N>.
    if s.len() > N {
        unsafe { std::hint::unreachable_unchecked() };
    }
    s
}

impl<T: Encode, const N: usize> Encoder<ArrayVec<T, N>> for VecEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &ArrayVec<T, N>) {
        self.encode(as_slice_assert_len(t));
    }
    #[inline(always)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a ArrayVec<T, N>> + Clone)
    where
        ArrayVec<T, N>: 'a,
    {
        self.encode_vectored(i.map(as_slice_assert_len));
    }
}
impl<T: Encode, const N: usize> Encode for ArrayVec<T, N> {
    type Encoder = VecEncoder<T>;
}

pub struct ArrayVecDecoder<'a, T: Decode<'a>, const N: usize>(VecDecoder<'a, T>);
// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>, const N: usize> Default for ArrayVecDecoder<'a, T, N> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<'a, T: Decode<'a>, const N: usize> View<'a> for ArrayVecDecoder<'a, T, N> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)?;
        // Safety: `length` was same length passed to populate.
        if unsafe { self.0.lengths.any_greater_than::<N>(length) } {
            return err("invalid ArrayVec");
        }
        Ok(())
    }
}
impl<'a, T: Decode<'a>, const N: usize> Decoder<'a, ArrayVec<T, N>> for ArrayVecDecoder<'a, T, N> {
    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<ArrayVec<T, N>>) {
        // Safety: We've ensured self.lengths.max_len() <= N in populate.
        unsafe {
            let av = out.write(ArrayVec::new());
            let n = self.0.lengths.decode();
            for i in 0..n {
                self.0
                    .elements
                    .decode_in_place(&mut *(av.as_mut_ptr().add(i) as *mut MaybeUninit<T>));
            }
            av.set_len(n);
        }
    }
}
impl<'a, T: Decode<'a>, const N: usize> Decode<'a> for ArrayVec<T, N> {
    type Decoder = ArrayVecDecoder<'a, T, N>;
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use arrayvec::{ArrayString, ArrayVec};

    // Smaller set of tests for ArrayString than ArrayVec they share VecEncoder/LengthDecoder.
    #[test]
    fn array_string() {
        let mut v = ArrayString::<2>::default();
        v.push('0');
        v.push('1');
        let b = encode(&v);
        assert!(decode::<ArrayString<1>>(&b).is_err());
        assert_eq!(decode::<ArrayString<2>>(&b).unwrap(), v);
        assert_eq!(decode::<ArrayString<3>>(&b).unwrap().as_str(), v.as_str());
        assert!(decode::<ArrayString<0>>(&encode(&ArrayString::<0>::default())).is_ok());
    }

    #[test]
    fn array_vec() {
        let mut v = ArrayVec::<u8, 2>::default();
        v.push(0);
        v.push(1);
        let b = encode(&v);
        assert!(decode::<ArrayVec<u8, 1>>(&b).is_err());
        assert_eq!(decode::<ArrayVec<u8, 2>>(&b).unwrap(), v);
        assert_eq!(
            decode::<ArrayVec<u8, 3>>(&b).unwrap().as_slice(),
            v.as_slice()
        );
        assert_eq!(
            decode::<ArrayVec<u8, 500>>(&b).unwrap().as_slice(),
            v.as_slice()
        );
        assert!(decode::<ArrayVec<u8, 0>>(&encode(&ArrayVec::<u8, 0>::default())).is_ok());

        // Make sure LengthDecoder::any_greater_than works on large lengths too.
        let mut v = ArrayVec::<u8, 500>::default();
        for i in 0..500 {
            v.push(i as u8);
        }
        let b = encode(&v);
        assert!(decode::<ArrayVec<u8, 499>>(&b).is_err());
        assert_eq!(decode::<ArrayVec<u8, 500>>(&b).unwrap(), v);
    }

    #[test]
    fn array_string_bug() {
        type T = ArrayString<1>;
        let mut v = T::default();
        v.push(' ');

        let mut buffer = crate::Buffer::new();
        // Put the buffer in a state where its ArrayStringDecoder's LengthDecoder has a len of 1.
        // If this didn't error, the decoder would end up with a len of 0 since all items would be consumed.
        buffer
            .decode::<Vec<T>>(&encode::<Vec<T>>(&vec![v])[..2])
            .unwrap_err();
        // Now decode 0 ArrayStrings. Previously LengthDecoder wouldn't get populated if StrDecoder
        // was passed `length` of 0. This results in a debug assert failing in
        // LengthDecoder::any_greater_than, since FastSlice::as_slice is called with len of 0, but
        // the FastSlice has a len of 1 from before.
        buffer.decode::<Vec<T>>(&encode::<Vec<T>>(&vec![])).unwrap();
    }
}
