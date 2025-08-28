use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::error::err;
use crate::fast::{CowSlice, NextUnchecked, PushUnchecked, VecImpl};
use crate::pack::{pack_bytes_less_than, unpack_bytes_less_than};
use crate::pack_ints::{pack_ints, unpack_ints, Int};
use alloc::vec::Vec;
use core::any::TypeId;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct VariantEncoder<T: Int, const N: usize>(VecImpl<T>);

impl<T: Int, const N: usize> Encoder<T> for VariantEncoder<T, N> {
    #[inline(always)]
    fn encode(&mut self, v: &T) {
        unsafe { self.0.push_unchecked(*v) };
    }
}

impl<T: Int, const N: usize> Buffer for VariantEncoder<T, N> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        assert!(N >= 2);
        if TypeId::of::<T>() != TypeId::of::<u8>() {
            pack_ints(self.0.as_mut_slice(), out);
        } else {
            pack_bytes_less_than::<N>(bytemuck::must_cast_slice::<T, u8>(self.0.as_slice()), out);
        };
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}

pub struct VariantDecoder<'a, T: Int, const N: usize, const HISTOGRAM: usize> {
    variants: CowSlice<'a, T::Une>,
    // `HISTOGRAM` is 0 for C style (fieldless) enums.
    histogram: [usize; HISTOGRAM],
}

// [(); N] doesn't implement Default.
impl<T: Int, const N: usize, const HISTOGRAM: usize> Default
    for VariantDecoder<'_, T, N, HISTOGRAM>
{
    fn default() -> Self {
        Self {
            variants: Default::default(),
            histogram: core::array::from_fn(|_| 0),
        }
    }
}

// C style enums (`HISTOGRAM` = 0) don't require length, so we
// can skip making a histogram for them.
impl<'a, T: Int, const N: usize> VariantDecoder<'a, T, N, N> {
    pub fn length(&self, variant_index: u8) -> usize {
        self.histogram[variant_index as usize]
    }
}

impl<'a, T: Int + Into<usize>, const N: usize, const HISTOGRAM: usize> View<'a>
    for VariantDecoder<'a, T, N, HISTOGRAM>
{
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        assert!(N >= 2);
        if TypeId::of::<T>() != TypeId::of::<u8>() {
            unpack_ints::<T>(input, length, &mut self.variants)?;

            /// Checks that `unpacked` ints are less than `N`, hopefully
            /// without a branch instruction for every int.
            fn check_less_than<T: Int + Into<usize>, const N: usize>(
                unpacked: &[T::Une],
            ) -> Result<()> {
                if 2u64.pow(core::mem::size_of::<T>() as u32 * 8) - 1 > N as u64
                    && unpacked
                        .iter()
                        .copied()
                        .map(T::from_unaligned)
                        .max()
                        .map(Into::into)
                        .unwrap_or(0)
                        >= N
                {
                    return err("invalid enum variant index");
                }
                Ok(())
            }

            check_less_than::<T, N>(unsafe { self.variants.as_slice(length) })?;
        } else {
            let out = self.variants.cast_mut::<u8>();
            self.histogram = unpack_bytes_less_than::<N, HISTOGRAM>(input, length, out)?;
        }
        Ok(())
    }
}

impl<'a, T: Int + Into<usize>, const N: usize, const HISTOGRAM: usize> Decoder<'a, T>
    for VariantDecoder<'a, T, N, HISTOGRAM>
{
    // Guaranteed to output numbers less than N.
    #[inline(always)]
    fn decode(&mut self) -> T {
        bytemuck::must_cast(unsafe { self.variants.mut_slice().next_unchecked() })
    }
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode, Decode, Encode};
    use alloc::vec::Vec;

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

#[cfg(test)]
mod test2 {
    use crate::{decode, encode, Decode, Encode};
    use alloc::vec::Vec;

    #[cfg_attr(not(test), rustfmt::skip)]
    #[derive(Encode, Decode, Debug, PartialEq)]
    pub enum Enum300 {
        V1, V2, V3, V4, V5, V6, V7, V8, V9, V10,
        V11, V12, V13, V14, V15, V16, V17, V18, V19, V20,
        V21, V22, V23, V24, V25, V26, V27, V28, V29, V30,
        V31, V32, V33, V34, V35, V36, V37, V38, V39, V40,
        V41, V42, V43, V44, V45, V46, V47, V48, V49, V50,
        V51, V52, V53, V54, V55, V56, V57, V58, V59, V60,
        V61, V62, V63, V64, V65, V66, V67, V68, V69, V70,
        V71, V72, V73, V74, V75, V76, V77, V78, V79, V80,
        V81, V82, V83, V84, V85, V86, V87, V88, V89, V90,
        V91, V92, V93, V94, V95, V96, V97, V98, V99, V100,
        V101, V102, V103, V104, V105, V106, V107, V108, V109, V110,
        V111, V112, V113, V114, V115, V116, V117, V118, V119, V120,
        V121, V122, V123, V124, V125, V126, V127, V128, V129, V130,
        V131, V132, V133, V134, V135, V136, V137, V138, V139, V140,
        V141, V142, V143, V144, V145, V146, V147, V148, V149, V150,
        V151, V152, V153, V154, V155, V156, V157, V158, V159, V160,
        V161, V162, V163, V164, V165, V166, V167, V168, V169, V170,
        V171, V172, V173, V174, V175, V176, V177, V178, V179, V180,
        V181, V182, V183, V184, V185, V186, V187, V188, V189, V190,
        V191, V192, V193, V194, V195, V196, V197, V198, V199, V200,
        V201, V202, V203, V204, V205, V206, V207, V208, V209, V210,
        V211, V212, V213, V214, V215, V216, V217, V218, V219, V220,
        V221, V222, V223, V224, V225, V226, V227, V228, V229, V230,
        V231, V232, V233, V234, V235, V236, V237, V238, V239, V240,
        V241, V242, V243, V244, V245, V246, V247, V248, V249, V250,
        V251, V252, V253, V254, V255, V256, V257, V258, V259, V260,
        V261, V262, V263, V264, V265, V266, V267, V268, V269, V270,
        V271, V272, V273, V274, V275, V276, V277, V278, V279, V280,
        V281, V282, V283, V284, V285, V286, V287, V288, V289, V290,
        V291, V292, V293, V294, V295, V296, V297, V298, V299, V300,
    }

    #[allow(unused)]
    #[test]
    fn test_large_c_style_enum() {
        assert!(matches!(decode(&encode(&Enum300::V42)), Ok(Enum300::V42)));
        assert!(matches!(decode(&encode(&Enum300::V300)), Ok(Enum300::V300)));
    }

    fn bench_data() -> Vec<Enum300> {
        crate::random_data(1000)
            .into_iter()
            .map(|v: u16| unsafe { core::mem::transmute_copy::<_, Enum300>(&(v % 300)) })
            .collect()
    }
    crate::bench_encode_decode!(enum_300_variants_vec: Vec<_>);
}
