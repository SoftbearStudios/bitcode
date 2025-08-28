use crate::coder::{Buffer, Decoder, Encoder, View};
use crate::derive::variant::{VariantDecoder, VariantEncoder};
use crate::derive::{Decode, Encode};
use crate::error::Error;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;

pub struct ResultEncoder<T: Encode, E: Encode> {
    variants: VariantEncoder<u8, 2>,
    ok: T::Encoder,
    err: E::Encoder,
}

// Can't derive since it would bound T + E: Default.
impl<T: Encode, E: Encode> Default for ResultEncoder<T, E> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            ok: Default::default(),
            err: Default::default(),
        }
    }
}

impl<T: Encode, E: Encode> Encoder<Result<T, E>> for ResultEncoder<T, E> {
    #[inline(always)]
    fn encode(&mut self, t: &Result<T, E>) {
        self.variants.encode(&(t.is_err() as u8));
        match t {
            Ok(t) => {
                self.ok.reserve(NonZeroUsize::new(1).unwrap());
                self.ok.encode(t);
            }
            Err(t) => {
                self.err.reserve(NonZeroUsize::new(1).unwrap());
                self.err.encode(t);
            }
        }
    }
    // TODO implement encode_vectored if we can avoid lots of code duplication with OptionEncoder.
}

impl<T: Encode, E: Encode> Buffer for ResultEncoder<T, E> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.variants.collect_into(out);
        self.ok.collect_into(out);
        self.err.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.variants.reserve(additional);
        // We don't know how many are Ok or Err, so we can't reserve more.
    }
}

pub struct ResultDecoder<'a, T: Decode<'a>, E: Decode<'a>> {
    variants: VariantDecoder<'a, u8, 2, 2>,
    ok: T::Decoder,
    err: E::Decoder,
}

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>, E: Decode<'a>> Default for ResultDecoder<'a, T, E> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            ok: Default::default(),
            err: Default::default(),
        }
    }
}

impl<'a, T: Decode<'a>, E: Decode<'a>> View<'a> for ResultDecoder<'a, T, E> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<(), Error> {
        self.variants.populate(input, length)?;
        self.ok.populate(input, self.variants.length(0))?;
        self.err.populate(input, self.variants.length(1))
    }
}

impl<'a, T: Decode<'a>, E: Decode<'a>> Decoder<'a, Result<T, E>> for ResultDecoder<'a, T, E> {
    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<Result<T, E>>) {
        if self.variants.decode() == 0 {
            out.write(Ok(self.ok.decode()));
        } else {
            out.write(Err(self.err.decode()));
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    fn bench_data() -> Vec<Result<u32, u8>> {
        crate::random_data::<(bool, u32, u8)>(1000)
            .into_iter()
            .map(|(is_ok, ok, err)| if is_ok { Ok(ok) } else { Err(err) })
            .collect()
    }
    crate::bench_encode_decode!(result_vec: Vec<_>);
}
