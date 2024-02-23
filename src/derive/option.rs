use crate::coder::{Buffer, Decoder, Encoder, Result, View, MAX_VECTORED_CHUNK};
use crate::derive::variant::{VariantDecoder, VariantEncoder};
use crate::derive::{Decode, Encode};
use crate::fast::{FastArrayVec, PushUnchecked};
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;

#[derive(Debug)]
pub struct OptionEncoder<T: Encode> {
    variants: VariantEncoder<2>,
    some: T::Encoder,
}

// Can't derive since it would bound T: Default.
impl<T: Encode> Default for OptionEncoder<T> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            some: Default::default(),
        }
    }
}

impl<T: Encode> Encoder<Option<T>> for OptionEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &Option<T>) {
        self.variants.encode(&(t.is_some() as u8));
        if let Some(t) = t {
            self.some.reserve(NonZeroUsize::new(1).unwrap());
            self.some.encode(t);
        }
    }

    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a Option<T>> + Clone)
    where
        Option<T>: 'a,
    {
        // Types with many vectorized encoders benefit from a &[&T] since encode_vectorized is still
        // faster even with the extra indirection. TODO vectored encoder count >= 8 instead of size_of.
        if std::mem::size_of::<T>() >= 64 {
            let mut uninit = MaybeUninit::uninit();
            let mut refs = FastArrayVec::<_, MAX_VECTORED_CHUNK>::new(&mut uninit);

            for t in i {
                self.variants.encode(&(t.is_some() as u8));
                if let Some(t) = t {
                    // Safety: Even if all `Some` won't write more than MAX_VECTORED_CHUNK elements.
                    unsafe { refs.push_unchecked(t) };
                }
            }

            let refs = refs.as_slice();
            let Some(some_count) = NonZeroUsize::new(refs.len()) else {
                return;
            };
            self.some.reserve(some_count);
            self.some.encode_vectored(refs.iter().copied());
        } else {
            let mut some_count = 0;
            for t in i.clone() {
                let is_some = t.is_some() as u8;
                some_count += is_some as usize;
                self.variants.encode(&is_some);
            }

            let Some(some_sum) = NonZeroUsize::new(some_count) else {
                return;
            };
            self.some.reserve(some_sum);
            for t in i.flatten() {
                self.some.encode(t);
            }
        }
    }
}

impl<T: Encode> Buffer for OptionEncoder<T> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.variants.collect_into(out);
        self.some.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.variants.reserve(additional);
        // We don't know how many are Some, so we can't reserve more.
    }
}

#[derive(Debug)]
pub struct OptionDecoder<'a, T: Decode<'a>> {
    variants: VariantDecoder<'a, 2, false>,
    some: T::Decoder,
}

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>> Default for OptionDecoder<'a, T> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            some: Default::default(),
        }
    }
}

impl<'a, T: Decode<'a>> View<'a> for OptionDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.variants.populate(input, length)?;
        self.some.populate(input, self.variants.length(1))
    }
}

impl<'a, T: Decode<'a>> Decoder<'a, Option<T>> for OptionDecoder<'a, T> {
    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<Option<T>>) {
        if self.variants.decode() != 0 {
            out.write(Some(self.some.decode()));
        } else {
            out.write(None);
        }
    }
}

#[cfg(test)]
mod tests {
    #[rustfmt::skip]
    fn bench_data() -> Vec<Option<(u64, u32, u8, i32, u64, u32, u8, i32, u64, (u32, u8, i32, u64, u32, u8, i32))>> {
        crate::random_data(1000)
    }
    crate::bench_encode_decode!(option_vec: Vec<_>);
}
