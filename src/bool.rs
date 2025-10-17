use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, SliceImpl, Unaligned, VecImpl};
use crate::pack::{pack_bools, unpack_bools};
use alloc::vec::Vec;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct BoolEncoder(VecImpl<bool>);

impl Encoder<bool> for BoolEncoder {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<bool>> {
        Some(&mut self.0)
    }

    #[inline(always)]
    fn encode(&mut self, t: &bool) {
        unsafe { self.0.push_unchecked(*t) };
    }
}

impl Buffer for BoolEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        pack_bools(self.0.as_slice(), out);
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}

#[derive(Default)]
pub struct BoolDecoder<'a>(CowSlice<'a, bool>);

impl<'a> View<'a> for BoolDecoder<'a> {
    fn populate(&mut self, input: &mut &'_ [u8], length: usize) -> Result<()> {
        unpack_bools(input, length, &mut self.0)?;
        Ok(())
    }
}

impl<'a> Decoder<'a, bool> for BoolDecoder<'a> {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut SliceImpl<'_, Unaligned<bool>>> {
        // Safety: `Unaligned<bool>` is equivalent to bool since it's a `#[repr(C, packed)]` wrapper
        // around bool and both have size/align of 1.
        unsafe { Some(core::mem::transmute(self.0.mut_slice())) }
    }

    #[inline(always)]
    fn decode(&mut self) -> bool {
        unsafe { self.0.mut_slice().next_unchecked() }
    }
}

#[cfg(test)]
mod test {
    use alloc::vec::Vec;

    fn bench_data() -> Vec<bool> {
        (0..=1000).map(|_| false).collect()
    }
    crate::bench_encode_decode!(bool_vec: Vec<_>);
}

#[cfg(test)]
mod test2 {
    use alloc::vec::Vec;

    fn bench_data() -> Vec<Vec<bool>> {
        crate::random_data::<u8>(125)
            .into_iter()
            .map(|n| {
                let n = 1 + n / 16;
                (0..n).map(|_| false).collect()
            })
            .collect()
    }
    crate::bench_encode_decode!(bool_vecs: Vec<Vec<_>>);
}
