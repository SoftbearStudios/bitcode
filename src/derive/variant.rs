use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, VecImpl};
use crate::pack::{pack_bytes_less_than, unpack_bytes_less_than};
use std::num::NonZeroUsize;

#[derive(Default)]
pub struct VariantEncoder<const N: usize>(VecImpl<u8>);

impl<const N: usize> Encoder<u8> for VariantEncoder<N> {
    #[inline(always)]
    fn encode(&mut self, v: &u8) {
        unsafe { self.0.push_unchecked(*v) };
    }
}

impl<const N: usize> Buffer for VariantEncoder<N> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        assert!(N >= 2);
        pack_bytes_less_than::<N>(self.0.as_slice(), out);
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}

pub struct VariantDecoder<'a, const N: usize, const C_STYLE: bool> {
    variants: CowSlice<'a, u8>,
    histogram: [usize; N], // Not required if C_STYLE. TODO don't reserve space for it.
}

// [(); N] doesn't implement Default.
impl<const N: usize, const C_STYLE: bool> Default for VariantDecoder<'_, N, C_STYLE> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            histogram: std::array::from_fn(|_| 0),
        }
    }
}

// C style enums don't require length, so we can skip making a histogram for them.
impl<'a, const N: usize> VariantDecoder<'a, N, false> {
    pub fn length(&self, variant_index: u8) -> usize {
        self.histogram[variant_index as usize]
    }
}

impl<'a, const N: usize, const C_STYLE: bool> View<'a> for VariantDecoder<'a, N, C_STYLE> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        assert!(N >= 2);
        if C_STYLE {
            unpack_bytes_less_than::<N, 0>(input, length, &mut self.variants)?;
        } else {
            self.histogram = unpack_bytes_less_than::<N, N>(input, length, &mut self.variants)?;
        }
        Ok(())
    }
}

impl<'a, const N: usize, const C_STYLE: bool> Decoder<'a, u8> for VariantDecoder<'a, N, C_STYLE> {
    // Guaranteed to output numbers less than N.
    #[inline(always)]
    fn decode(&mut self) -> u8 {
        unsafe { self.variants.mut_slice().next_unchecked() }
    }
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode, Decode, Encode};

    #[allow(unused)]
    #[test]
    fn test_c_style_enum() {
        #[derive(Encode, Decode)]
        enum Enum1 {
            A,
            B,
            C,
            D,
            E,
            F,
        }
        #[derive(Decode)]
        enum Enum2 {
            A,
            B,
            C,
            D,
            E,
        }
        // 5 and 6 element enums serialize the same, so we can use them to test variant bounds checking.
        assert!(matches!(decode(&encode(&Enum1::A)), Ok(Enum2::A)));
        assert!(decode::<Enum2>(&encode(&Enum1::F)).is_err());
        assert!(matches!(decode(&encode(&Enum1::F)), Ok(Enum1::F)));
    }

    #[allow(unused)]
    #[test]
    fn test_rust_style_enum() {
        #[derive(Encode, Decode)]
        enum Enum1 {
            A(u8),
            B,
            C,
            D,
            E,
            F,
        }
        #[derive(Decode)]
        enum Enum2 {
            A(u8),
            B,
            C,
            D,
            E,
        }
        // 5 and 6 element enums serialize the same, so we can use them to test variant bounds checking.
        assert!(matches!(decode(&encode(&Enum1::A(1))), Ok(Enum2::A(1))));
        assert!(decode::<Enum2>(&encode(&Enum1::F)).is_err());
        assert!(matches!(decode(&encode(&Enum1::F)), Ok(Enum1::F)));
    }

    #[derive(Debug, PartialEq, Encode, Decode)]
    enum BoolEnum {
        True,
        False,
    }
    fn bench_data() -> Vec<BoolEnum> {
        crate::random_data(1000)
            .into_iter()
            .map(|v| if v { BoolEnum::True } else { BoolEnum::False })
            .collect()
    }
    crate::bench_encode_decode!(bool_enum_vec: Vec<_>);
}
