use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, VecImpl};
use crate::pack::{pack_bytes, unpack_bytes};
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct VariantEncoder {
    data: VecImpl<u8>,
}

impl<Index> Encoder for VariantEncoder {
    #[inline(always)]
    fn encode(&mut self, v: &u8) {
        unsafe { self.data.push_unchecked(*v) };
    }
}

impl Buffer for VariantEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        pack_bytes(self.data.as_mut_slice(), out);
        self.data.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.data.reserve(additional.get());
    }
}

#[derive(Default)]
pub struct VariantDecoder<'a> {
    variants: CowSlice<'a, u8>,
    histogram: Vec<usize>,
    spooky: PhantomData<&'a ()>,
}

impl VariantDecoder<'_> {
    pub fn length(&self, variant_index: u8) -> usize {
        self.histogram[variant_index as usize]
    }

    /// Returns the max variant index if there were any variants.
    pub fn max_variant_index(&self) -> Option<u8> {
        self.histogram.len().checked_sub(1).map(|v| v as u8)
    }
}

impl<'a> View<'a> for VariantDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        unpack_bytes(input, length, &mut self.variants)?;
        // Safety: unpack_bytes just initialized self.variants with length of `length`.
        let variants = unsafe { self.variants.as_slice(length) };

        let histogram = crate::histogram::histogram(variants);
        let len = histogram
            .iter()
            .copied()
            .rposition(|v| v != 0)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.histogram.clear();
        self.histogram.extend_from_slice(&histogram[..len]);
        Ok(())
    }
}

impl<'a> Decoder<'a, u8> for VariantDecoder<'a> {
    fn decode(&mut self) -> u8 {
        unsafe { self.variants.mut_slice().next_unchecked() }
    }
}
