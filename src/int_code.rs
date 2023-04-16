use crate::de::read::Read;
use crate::nightly::ilog2;
use crate::ser::write::Write;
use crate::{Result, E};

pub trait NumericEncoding {
    /// Encode an unsigned word size value.
    fn encode_word<W: Write + ?Sized>(&self, target: &mut W, value: usize) -> Result<()>;

    /// Decode an unsigned word size value.
    fn decode_word<R: Read + ?Sized>(&self, source: &mut R) -> Result<usize>;

    /// Encode an arbitrary numerical value.
    ///
    /// Only works for types that are at most as wide as the pointer size.
    fn encode<N: Num, W: Write + ?Sized>(&self, target: &mut W, value: N) -> Result<()>;

    /// Decode an arbitrary numerical value.
    ///
    /// Only works for types that are at most as wide as the pointer size.
    fn decode<N: Num, R: Read + ?Sized>(&self, source: &mut R) -> Result<N>;
}

/// A [NumericEncoding] that encodes word sized values using Elias gamma code.
///
/// Other numeric values are encoded with their full bit representation.
#[derive(Debug, Clone, Copy)]
pub struct DefaultNumericEncoding;

impl NumericEncoding for DefaultNumericEncoding {
    fn encode_word<W: Write + ?Sized>(&self, target: &mut W, value: usize) -> Result<()> {
        EliasGammaEncoding::STANDALONE.encode_usize(target, value)
    }

    fn decode_word<R: Read + ?Sized>(&self, source: &mut R) -> Result<usize> {
        EliasGammaEncoding::STANDALONE.decode_usize(source)
    }

    fn encode<N: Num, W: Write + ?Sized>(&self, target: &mut W, value: N) -> Result<()> {
        target.write_bits(value.to_usize() as u64, N::BITS as usize);
        Ok(())
    }

    fn decode<N: Num, R: Read + ?Sized>(&self, source: &mut R) -> Result<N> {
        Ok(N::from_usize(source.read_bits(N::BITS as usize)? as usize))
    }
}

/// A [NumericEncoding] that encodes every numerical value using Elias gamma code.
#[derive(Debug, Clone, Copy)]
pub struct FullGammaEncoding;

impl NumericEncoding for FullGammaEncoding {
    fn encode_word<W: Write + ?Sized>(&self, target: &mut W, value: usize) -> Result<()> {
        EliasGammaEncoding::STANDALONE.encode_usize(target, value)
    }

    fn decode_word<R: Read + ?Sized>(&self, source: &mut R) -> Result<usize> {
        EliasGammaEncoding::STANDALONE.decode_usize(source)
    }

    fn encode<N: Num, W: Write + ?Sized>(&self, target: &mut W, value: N) -> Result<()> {
        self.encode_word(target, value.to_usize())
    }

    fn decode<N: Num, R: Read + ?Sized>(&self, source: &mut R) -> Result<N> {
        self.decode_word(source).map(N::from_usize)
    }
}

/// A [NumericEncoding] that encodes values using either Elias gamma encoding or their full bit representation.
///
/// If a value can be represented with Elias gamma encoding using less bits than its full representation,
/// then that encoding is used.
/// If the Elias gamma encoding would use more bits than the full representation of the type,
/// then the value is encoded as a single bit set to 1 followed by the full representation of the value.
///
/// The inflection point at which Elias gamma becomes less efficient is the square root of the maximum value of a type,
/// which is the same as saying when the bit length of a value is more than half of its bit size.
/// On `u8`, for example, the inflection point is at 16, so:
///
/// | Value | Binary      | Elias gamma           | Bounded gamma encoding |
/// |:----- |:------------|:----------------------|:-----------------------|
/// | `10`  | `0000 1010` | `000 1010`            | `010 1010`             |
/// | `15`  | `0000 1111` | `000 1111`            | `010 1111`             |
/// | `16`  | `0001 1111` | `0 0001 0000`         | `1 0001 0000`          |
/// | `255` | `1111 1111` | `000 0000 1111 1111`  | `1 1111 1111`          |
#[derive(Debug, Clone, Copy)]
pub struct BoundedGammaEncoding;

impl NumericEncoding for BoundedGammaEncoding {
    fn encode_word<W: Write + ?Sized>(&self, target: &mut W, value: usize) -> Result<()> {
        self.encode_usize(target, value, usize::BITS)
    }

    fn decode_word<R: Read + ?Sized>(&self, source: &mut R) -> Result<usize> {
        self.decode_usize(source, usize::BITS)
    }

    fn encode<N: Num, W: Write + ?Sized>(&self, target: &mut W, value: N) -> Result<()> {
        self.encode_usize(target, value.to_usize(), N::BITS)
    }

    fn decode<N: Num, R: Read + ?Sized>(&self, source: &mut R) -> Result<N> {
        self.decode_usize(source, N::BITS).map(N::from_usize)
    }
}

impl BoundedGammaEncoding {
    /// Encodes a `usize` with maximum amount of bits stored.
    fn encode_usize(
        &self,
        target: &mut (impl Write + ?Sized),
        value: usize,
        bit_max: u32,
    ) -> Result<()> {
        let bit_size = usize::BITS - value.leading_zeros();
        debug_assert!(
            bit_size <= bit_max,
            "invalid value {value} to encode, larger than 2^bit_max (bit_size={bit_size})",
            value = value,
            bit_size = bit_size,
        );

        if bit_size > (bit_max / 2) {
            target.write_bit(true);
            target.write_bits(value as u64, bit_max as usize);

            Ok(())
        } else {
            EliasGammaEncoding::SHARED.encode_usize(target, value)
        }
    }

    /// Decodes a `usize` knowing the maximum amount of bits stored.
    fn decode_usize(&self, source: &mut (impl Read + ?Sized), bit_max: u32) -> Result<usize> {
        let skip_gamma = source.read_bit()?;

        if skip_gamma {
            Ok(source.read_bits(bit_max as usize)? as usize)
        } else {
            EliasGammaEncoding::SHARED.decode_usize(source)
        }
    }
}

/// A dynamically selected [NumericEncoding].
#[derive(Debug, Clone, Copy)]
pub enum DynamicEncoding {
    /// Dispatches to [DefaultNumericEncoding].
    Default,
    /// Dispatches to [FullGammaEncoding].
    FullGamma,
    /// Dispatches to [BoundedGammaEncoding].
    BoundedGamma,
}

impl DynamicEncoding {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] = [Self::Default, Self::FullGamma, Self::BoundedGamma];
}

impl NumericEncoding for DynamicEncoding {
    fn encode_word<W: Write + ?Sized>(&self, target: &mut W, value: usize) -> Result<()> {
        match self {
            DynamicEncoding::Default => DefaultNumericEncoding.encode_word(target, value),
            DynamicEncoding::FullGamma => FullGammaEncoding.encode_word(target, value),
            DynamicEncoding::BoundedGamma => BoundedGammaEncoding.encode_word(target, value),
        }
    }

    fn decode_word<R: Read + ?Sized>(&self, source: &mut R) -> Result<usize> {
        match self {
            DynamicEncoding::Default => DefaultNumericEncoding.decode_word(source),
            DynamicEncoding::FullGamma => FullGammaEncoding.decode_word(source),
            DynamicEncoding::BoundedGamma => BoundedGammaEncoding.decode_word(source),
        }
    }

    fn encode<N: Num, W: Write + ?Sized>(&self, target: &mut W, value: N) -> Result<()> {
        match self {
            DynamicEncoding::Default => DefaultNumericEncoding.encode(target, value),
            DynamicEncoding::FullGamma => FullGammaEncoding.encode(target, value),
            DynamicEncoding::BoundedGamma => BoundedGammaEncoding.encode(target, value),
        }
    }

    fn decode<N: Num, R: Read + ?Sized>(&self, source: &mut R) -> Result<N> {
        match self {
            DynamicEncoding::Default => DefaultNumericEncoding.decode(source),
            DynamicEncoding::FullGamma => FullGammaEncoding.decode(source),
            DynamicEncoding::BoundedGamma => BoundedGammaEncoding.decode(source),
        }
    }
}

/// [Elias gamma encoding](https://en.wikipedia.org/wiki/Elias_gamma_coding).
struct EliasGammaEncoding {
    share_zero_tag: bool,
}

impl EliasGammaEncoding {
    const SHARED: Self = Self {
        share_zero_tag: true,
    };
    const STANDALONE: Self = Self {
        share_zero_tag: false,
    };

    /// Returns the offset for Elias gamma encoding.
    const fn offset(&self) -> usize {
        if self.share_zero_tag {
            // Offset by two so every value has at least one leading zero.
            2
        } else {
            // Elias gamma cannot encode zero, so offset by one.
            1
        }
    }

    /// Encode a `usize` using Elias gamma encoding.
    fn encode_usize(&self, target: &mut (impl Write + ?Sized), value: usize) -> Result<()> {
        let v = value
            .checked_add(self.offset())
            // We don't support usize::MAX using gamma encoding because it would add more code
            // and it's only useful for ZST.
            .ok_or(E::NotSupported("value must be < usize::MAX for gamma encoding").e())?;

        let zeros = ilog2(v) as usize;
        let bit_count = zeros * 2 + 1;

        if bit_count <= 64 {
            let bits = (v as u64).reverse_bits() >> (u64::BITS as usize - bit_count);
            target.write_bits(bits, bit_count as usize);
        } else {
            #[cold]
            fn slow(target: &mut (impl Write + ?Sized), v: usize) {
                let zeros = ilog2(v) as usize;
                target.write_bits(0, zeros);

                let integer_bits = zeros + 1;
                let lz = usize::BITS as usize - integer_bits;

                let bits = (v.reverse_bits() >> lz) as u64;
                target.write_bits(bits, integer_bits);
            }

            slow(target, v);
        }
        Ok(())
    }

    /// Decode a `usize` using Elias gamma encoding.
    fn decode_usize(&self, source: &mut (impl Read + ?Sized)) -> Result<usize> {
        let max_zeros = (usize::BITS - 1) as usize;
        let zeros = {
            let read_zeros = source
                .read_zeros(max_zeros)
                .map_err(|e| e.map_invalid("gamma usize value"))?;

            if self.share_zero_tag {
                // A leading zero was read before decoding, that zero is shared as both a tag
                // and part of the code.
                read_zeros + 1
            } else {
                read_zeros
            }
        };

        let integer_bits = zeros + 1;
        let v = source.read_bits(integer_bits)?;

        let lz = u64::BITS as usize - integer_bits;
        let v = (v << lz).reverse_bits();

        // An offset is added when encoding, so subtract the same offset.
        Ok((v as usize) - self.offset())
    }
}

pub trait Num: std::fmt::Debug + std::fmt::Binary {
    // Named BIT_SIZE instead of BITS as to not conflict in implementation.
    const BITS: u32;

    fn to_usize(self) -> usize;
    fn from_usize(val: usize) -> Self;
}

impl Num for usize {
    const BITS: u32 = usize::BITS;

    fn to_usize(self) -> usize {
        self
    }

    fn from_usize(val: usize) -> Self {
        val
    }
}

macro_rules! impl_num {
    ($t:ty, $mask:ty) => {
        impl Num for $t {
            const BITS: u32 = <$t>::BITS;

            #[inline(always)]
            fn to_usize(self) -> usize {
                // Apply a mask to ensure the compiler zero-extends the bits instead of
                // sign-extending.
                // See: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=c76563e48522ce708d521cb4f9ddf29f
                //
                // The result is either a `MOV` or `MOVZX` instruction, both run on a single cycle
                // if the operands are registers, which they are for the encoding code above.
                // https://godbolt.org/z/vKoPxxsPe
                self as $mask as usize
            }

            #[inline(always)]
            fn from_usize(val: usize) -> Self {
                val as $t
            }
        }
    };
}

impl_num!(u8, u8);
impl_num!(u16, u16);
impl_num!(u32, u32);
#[cfg(target_pointer_width = "64")]
impl_num!(u64, u64);

// TODO: Optimize negative number for values close to zero instead of close to MIN.
impl_num!(i8, u8);
impl_num!(i16, u16);
impl_num!(i32, u32);
#[cfg(target_pointer_width = "64")]
impl_num!(i64, u64);

#[cfg(test)]
mod tests {
    use super::*;

    mod word {
        use super::*;

        macro_rules! test_word {
            ($name:ident, $val:expr) => {
                mod $name {
                    use super::*;
                    use crate::de::read::{ReadWith, ReadWithImpl};
                    use crate::ser::write::{WriteWith, WriteWithImpl};

                    #[test]
                    fn default_encoding() {
                        let mut writer = WriteWithImpl::default();

                        let data: usize = $val;
                        DefaultNumericEncoding
                            .encode_word(&mut writer, data)
                            .unwrap();
                        let decoded: usize = DefaultNumericEncoding
                            .decode_word(&mut ReadWithImpl::from_inner(
                                writer.into_inner().as_slice(),
                            ))
                            .unwrap();

                        assert_eq!(decoded, data);
                    }

                    #[test]
                    fn full_gamma_encoding() {
                        let mut writer = WriteWithImpl::default();

                        let data: usize = $val;
                        FullGammaEncoding.encode_word(&mut writer, data).unwrap();
                        let decoded: usize = FullGammaEncoding
                            .decode_word(&mut ReadWithImpl::from_inner(
                                writer.into_inner().as_slice(),
                            ))
                            .unwrap();

                        assert_eq!(decoded, data);
                    }

                    #[test]
                    fn bounded_gamma_encoding() {
                        let mut writer = WriteWithImpl::default();

                        let data: usize = $val;
                        BoundedGammaEncoding.encode_word(&mut writer, data).unwrap();
                        let decoded: usize = BoundedGammaEncoding
                            .decode_word(&mut ReadWithImpl::from_inner(
                                writer.into_inner().as_slice(),
                            ))
                            .unwrap();

                        assert_eq!(decoded, data);
                    }

                    #[test]
                    fn dynamic_encoding() {
                        for encoding in DynamicEncoding::ALL {
                            let mut writer = WriteWithImpl::default();

                            let data: usize = $val;
                            encoding.encode_word(&mut writer, data).unwrap();
                            let decoded: usize = encoding
                                .decode_word(&mut ReadWithImpl::from_inner(
                                    writer.into_inner().as_slice(),
                                ))
                                .unwrap();

                            assert_eq!(decoded, data);
                        }
                    }
                }
            };
        }

        test_word!(zero, 0);
        test_word!(one, 1);
        test_word!(two, 2);
        test_word!(thousand, 1000);
        test_word!(mid, usize::MAX >> (usize::BITS / 2));
        test_word!(max, usize::MAX - 1);
    }

    macro_rules! test_encodings {
        ($name:ident, $ty:ty, $val:expr) => {
            mod $name {
                use super::*;
                use crate::de::read::{ReadWith, ReadWithImpl};
                use crate::ser::write::{WriteWith, WriteWithImpl};

                #[test]
                fn default_encoding() {
                    let mut writer = WriteWithImpl::default();

                    let data: $ty = $val;
                    DefaultNumericEncoding.encode(&mut writer, data).unwrap();
                    let decoded: $ty = DefaultNumericEncoding
                        .decode(&mut ReadWithImpl::from_inner(
                            writer.into_inner().as_slice(),
                        ))
                        .unwrap();

                    assert_eq!(decoded, data);
                }

                #[test]
                fn full_gamma_encoding() {
                    let mut writer = WriteWithImpl::default();

                    let data: $ty = $val;
                    FullGammaEncoding.encode(&mut writer, data).unwrap();
                    let decoded: $ty = FullGammaEncoding
                        .decode(&mut ReadWithImpl::from_inner(
                            writer.into_inner().as_slice(),
                        ))
                        .unwrap();

                    assert_eq!(decoded, data);
                }

                #[test]
                fn bounded_gamma_encoding() {
                    let mut writer = WriteWithImpl::default();

                    let data: $ty = $val;
                    BoundedGammaEncoding.encode(&mut writer, data).unwrap();
                    let decoded: $ty = BoundedGammaEncoding
                        .decode(&mut ReadWithImpl::from_inner(
                            writer.into_inner().as_slice(),
                        ))
                        .unwrap();

                    assert_eq!(decoded, data);
                }

                #[test]
                fn dynamic_encoding() {
                    for encoding in DynamicEncoding::ALL {
                        let mut writer = WriteWithImpl::default();

                        let data: $ty = $val;
                        encoding.encode(&mut writer, data).unwrap();
                        let decoded: $ty = encoding
                            .decode(&mut ReadWithImpl::from_inner(
                                writer.into_inner().as_slice(),
                            ))
                            .unwrap();

                        assert_eq!(decoded, data);
                    }
                }
            }
        };
    }

    macro_rules! test_multi_encodings {
        (($ty:ty) $($name:ident -> $val:expr),+,) => {
            $(
                test_encodings!($name, $ty, $val);
            )+
        };
    }

    test_multi_encodings! {
        (u8)
        zero_u8 -> 0,
        small_u8 -> 1,
        mid_u8 -> 16,
        min_u8 -> u8::MIN,
        max_u8 -> u8::MAX,
    }
    test_multi_encodings! {
        (u16)
        zero_u16 -> 0,
        small_u16 -> 1,
        mid_u16 -> u8::MAX as u16 + 1,
        min_u16 -> u16::MIN,
        max_u16 -> u16::MAX,
    }
    #[cfg(target_pointer_width = "32")]
    test_multi_encodings! {
        (u32)
        zero_u32 -> 0,
        small_u32 -> 1,
        mid_u32 -> u16::MAX as u32 + 1,
        min_u32 -> u32::MIN,
        max_u32 -> u32::MAX - 1,
    }
    #[cfg(target_pointer_width = "64")]
    test_multi_encodings! {
        (u32)
        zero_u32 -> 0,
        small_u32 -> 1,
        mid_u32 -> u16::MAX as u32 + 1,
        min_u32 -> u32::MIN,
        max_u32 -> u32::MAX,
    }
    #[cfg(target_pointer_width = "64")]
    test_multi_encodings! {
        (u64)
        zero_u64 -> 0,
        small_u64 -> 1,
        mid_u64 -> u32::MAX as u64 + 1,
        min_u64 -> u64::MIN,
        max_u64 -> u64::MAX - 1,
    }
    test_multi_encodings! {
        (usize)
        zero_usize -> 0,
        small_usize -> 1,
        min_usize -> usize::MIN,
        max_usize -> usize::MAX - 1,
    }

    test_multi_encodings! {
        (i8)
        zero_i8 -> 0,
        small_i8 -> 1,
        mid_i8 -> 16,
        min_i8 -> i8::MIN,
        max_i8 -> i8::MAX,
    }
    test_multi_encodings! {
        (i16)
        zero_i16 -> 0,
        small_i16 -> 1,
        mid_i16 -> u8::MAX as i16 + 1,
        min_i16 -> i16::MIN,
        max_i16 -> i16::MAX,
    }
    #[cfg(target_pointer_width = "64")]
    test_multi_encodings! {
        (i32)
        zero_i32 -> 0,
        small_i32 -> 1,
        mid_i32 -> u16::MAX as i32 + 1,
        min_i32 -> i32::MIN,
        max_i32 -> i32::MAX,
    }
}
