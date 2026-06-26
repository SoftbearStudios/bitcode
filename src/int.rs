use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::error::err;
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, SliceImpl, Unaligned, VecImpl};
use crate::pack_ints::{pack_ints, unpack_ints, Int};
use alloc::vec::Vec;
use bytemuck::{CheckedBitPattern, NoUninit, Pod};
use core::marker::PhantomData;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct IntEncoder<T>(VecImpl<T>);

/// Makes IntEncoder<u32> able to encode i32/f32/char.
impl<T: Int, P: NoUninit> Encoder<P> for IntEncoder<T> {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<P>> {
        use core::mem::*;
        assert_eq!(align_of::<T>(), align_of::<P>());
        assert_eq!(size_of::<T>(), size_of::<P>());
        // Safety: size/align are equal, T: Int implies Pod, and caller isn't reading P which may be NonZero.
        unsafe { Some(transmute(&mut self.0)) }
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
        self.0.reserve(additional.get());
    }
}

#[derive(Default)]
pub struct IntDecoder<'a, T: Int>(CowSlice<'a, T::Une>);

impl<'a, T: Int> IntDecoder<'a, T> {
    // For CheckedIntDecoder/LengthDecoder.
    pub(crate) fn borrowed_clone<'me: 'a>(&'me self) -> IntDecoder<'me, T> {
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
    fn as_primitive(&mut self) -> Option<&mut SliceImpl<'_, Unaligned<P>>> {
        Some(self.0.mut_slice().cast())
    }

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
        self.0.populate(input, length)?;

        let mut decoder = self.0.borrowed_clone();
        // Optimizes much better than Iterator::any.
        if (0..length)
            .filter(|_| !C::is_valid_bit_pattern(&decoder.decode()))
            .count()
            != 0
        {
            return err("invalid bit pattern");
        }
        Ok(())
    }
}

impl<'a, C: CheckedBitPattern + Send + Sync, I: Int> Decoder<'a, C> for CheckedIntDecoder<'a, C, I>
where
    <C as CheckedBitPattern>::Bits: Pod,
{
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut SliceImpl<'_, Unaligned<C>>> {
        self.0
            .as_primitive()
            .map(|p: &mut SliceImpl<'_, Unaligned<I>>| {
                let p = p.cast::<Unaligned<C::Bits>>();
                // Safety: `Unaligned<C::Bits>` and `Unaligned<C>` have the same layout and populate
                // ensured C's bit pattern is valid.
                unsafe { core::mem::transmute(p) }
            })
    }

    #[inline(always)]
    fn decode(&mut self) -> C {
        let v: I = self.0.decode();
        let v: C::Bits = bytemuck::must_cast(v);
        // Safety: C::Bits and C have the same layout and populate ensured C's bit pattern is valid.
        unsafe { core::mem::transmute_copy(&v) }
    }
}

/// Prevents callers of `ranged_int` from accessing `.0` in the same source file.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Private<T>(T);

impl<T> Private<T> {
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

#[allow(unused)]
macro_rules! ranged_int {
    ($type: ident, $int: ty, $lower: expr, $upper: expr) => {
        #[derive(Copy, Clone)]
        #[repr(transparent)]
        pub struct $type(crate::int::Private<$int>);
        // Safety: They have the same layout because of #[repr(transparent)].
        unsafe impl bytemuck::CheckedBitPattern for $type {
            type Bits = $int;
            #[inline(always)]
            fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
                const LOWER: $int = $lower;
                const UPPER: $int = $upper;
                (LOWER..=UPPER).contains(bits)
            }
        }
        impl $type {
            #[inline(always)]
            pub fn into_inner(self) -> $int {
                if !<Self as bytemuck::CheckedBitPattern>::is_valid_bit_pattern(
                    &self.0.into_inner(),
                ) {
                    // Safety: only created subject to `CheckedBitPattern`.
                    unsafe { core::hint::unreachable_unchecked() };
                }
                self.0.into_inner()
            }
        }

        impl<'a> crate::derive::Decode<'a> for $type {
            type Decoder = crate::int::CheckedIntDecoder<'a, $type, $int>;
        }
    };
}

#[allow(unused)]
pub(crate) use ranged_int;

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use alloc::vec::Vec;
    use core::num::NonZeroU32;

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
    use alloc::vec::Vec;

    fn bench_data() -> Vec<Vec<u16>> {
        crate::random_data::<u8>(125)
            .into_iter()
            .map(|n| (0..n / 54).map(|_| n as u16 * 255).collect())
            .collect()
    }
    crate::bench_encode_decode!(u16_vecs: Vec<Vec<_>>);
}
