use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::error::err;
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, VecImpl};
use crate::pack_ints::{pack_ints, unpack_ints, Int};
use bytemuck::{CheckedBitPattern, NoUninit, Pod};
use std::marker::PhantomData;
use std::num::NonZeroUsize;

#[derive(Debug, Default)]
pub struct IntEncoder<T>(VecImpl<T>);

/// Makes IntEncoder<u32> able to encode i32/f32/char.
impl<T: Int, P: NoUninit> Encoder<P> for IntEncoder<T> {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<P>> {
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<P>());
        // Safety: T and P are the same size, T is Pod, and we aren't reading P.
        let vec: &mut VecImpl<P> = unsafe { std::mem::transmute(&mut self.0) };
        Some(vec)
    }

    #[inline(always)]
    fn encode(&mut self, p: &P) {
        let t = bytemuck::must_cast(*p);
        unsafe { self.0.push_unchecked(t) };
    }
}

impl<T: Int> Buffer for IntEncoder<T> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        pack_ints(self.0.as_mut_slice(), out);
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get())
    }
}

#[derive(Debug, Default)]
pub struct IntDecoder<'a, T: Int>(CowSlice<'a, T::Une>);

impl<'a, T: Int> IntDecoder<'a, T> {
    // For CheckedIntDecoder.
    fn borrowed_clone<'me: 'a>(&'me self) -> IntDecoder<'me, T> {
        let mut cow = CowSlice::default();
        cow.set_borrowed_slice_impl(self.0.ref_slice().clone());
        Self(cow)
    }
}

impl<'a, T: Int> View<'a> for IntDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        unpack_ints::<T>(input, length, &mut self.0)?;
        Ok(())
    }
}

// Makes IntDecoder<u32> able to decode i32/f32 (but not char since it can fail).
impl<'a, T: Int, P: Pod> Decoder<'a, P> for IntDecoder<'a, T> {
    #[inline(always)]
    fn decode(&mut self) -> P {
        let v = unsafe { self.0.mut_slice().next_unchecked() };
        bytemuck::must_cast(v)
    }
}

/// For NonZeroU32, char, etc.
pub struct CheckedIntDecoder<'a, C, I: Int>(IntDecoder<'a, I>, PhantomData<C>);

// Can't bound C: Default since NonZeroU32/char don't implement it.
impl<C, I: Int> Default for CheckedIntDecoder<'_, C, I> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}

impl<'a, C: CheckedBitPattern, I: Int> View<'a> for CheckedIntDecoder<'a, C, I>
where
    <C as CheckedBitPattern>::Bits: Pod,
{
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        assert_eq!(std::mem::size_of::<C>(), std::mem::size_of::<I>());
        self.0.populate(input, length)?;

        let mut decoder = self.0.borrowed_clone();
        if (0..length).any(|_| !C::is_valid_bit_pattern(&decoder.decode())) {
            return err("invalid bit pattern");
        }
        Ok(())
    }
}

impl<'a, C: CheckedBitPattern, I: Int> Decoder<'a, C> for CheckedIntDecoder<'a, C, I>
where
    <C as CheckedBitPattern>::Bits: Pod,
{
    #[inline(always)]
    fn decode(&mut self) -> C {
        let i: I = self.0.decode();

        // Safety: populate ensures:
        // - C and I are of the same size.
        // - The checked bit pattern of C is valid.
        unsafe { std::mem::transmute_copy(&i) }
    }
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use std::num::NonZeroU32;

    #[test]
    fn non_zero_u32() {
        assert!(decode::<NonZeroU32>(&encode(&0u32)).is_err());
        assert!(decode::<NonZeroU32>(&encode(&1u32)).is_ok());
    }

    #[test]
    fn char_() {
        assert!(decode::<char>(&encode(&u32::MAX)).is_err());
        assert!(decode::<char>(&encode(&0u32)).is_ok());
    }

    fn bench_data() -> Vec<u16> {
        crate::random_data(1000)
    }
    crate::bench_encode_decode!(u16_vec: Vec<_>);
}

#[cfg(test)]
mod test2 {
    fn bench_data() -> Vec<Vec<u16>> {
        crate::random_data::<u8>(125)
            .into_iter()
            .map(|n| (0..n / 54).map(|_| n as u16 * 255).collect())
            .collect()
    }
    crate::bench_encode_decode!(u16_vecs: Vec<Vec<_>>);
}
