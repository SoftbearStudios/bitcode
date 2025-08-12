use crate::bool::BoolDecoder;
use crate::coder::{Decoder, Result, View};
use crate::consume::expect_eof;
use crate::error::{err, error, Error};
use crate::f32::F32Decoder;
use crate::int::IntDecoder;
use crate::length::LengthDecoder;
use crate::serde::guard::guard_zst;
use crate::serde::variant::VariantDecoder;
use crate::serde::{default_box_slice, get_mut_or_resize, type_changed};
use crate::str::StrDecoder;
use alloc::boxed::Box;
use alloc::vec::Vec;
use serde::de::{
    DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess, Visitor,
};
use serde::{Deserialize, Deserializer};

// Redefine Result from crate::coder::Result to std::result::Result since the former isn't public.
mod inner {
    use super::*;
    use core::result::Result;

    /// Deserializes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Deserialize`].
    ///
    /// **Warning:** The format is incompatible with [`encode`][`crate::encode`] and subject to
    /// change between major versions.
    pub fn deserialize<'de, T: Deserialize<'de>>(mut bytes: &'de [u8]) -> Result<T, Error> {
        let mut decoder = SerdeDecoder::Unspecified { length: 1 };
        let t = T::deserialize(DecoderWrapper {
            decoder: &mut decoder,
            input: &mut bytes,
        })?;
        expect_eof(bytes)?;
        Ok(t)
    }
}
pub use inner::deserialize;

enum SerdeDecoder<'a> {
    Bool(BoolDecoder<'a>),
    Enum((VariantDecoder<'a>, Vec<SerdeDecoder<'a>>)), // (variants, values)
    F32(F32Decoder<'a>),
    // We don't need signed integer decoders here because unsigned ones work the same.
    Map((LengthDecoder<'a>, Box<(SerdeDecoder<'a>, SerdeDecoder<'a>)>)), // (lengths, (keys, values))
    Seq((LengthDecoder<'a>, Box<SerdeDecoder<'a>>)),                     // (lengths, values)
    Str(StrDecoder<'a>),
    Tuple(Box<[SerdeDecoder<'a>]>), // [field0, field1, ..]
    U8(IntDecoder<'a, u8>),
    U16(IntDecoder<'a, u16>),
    U32(IntDecoder<'a, u32>),
    U64(IntDecoder<'a, u64>),
    U128(IntDecoder<'a, u128>),
    Unpopulated,
    Unspecified { length: usize },
}

impl Default for SerdeDecoder<'_> {
    fn default() -> Self {
        Self::Unpopulated
    }
}

impl<'a> View<'a> for SerdeDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        match self {
            Self::Bool(d) => d.populate(input, length),
            Self::Enum(d) => {
                d.0.populate(input, length)?;
                if let Some(max_variant_index) = d.0.max_variant_index() {
                    get_mut_or_resize(&mut d.1, max_variant_index as usize);
                    d.1.iter_mut()
                        .enumerate()
                        .try_for_each(|(i, variant)| variant.populate(input, d.0.length(i as u8)))
                } else {
                    Ok(())
                }
            }
            Self::F32(d) => d.populate(input, length),
            Self::Map(d) => {
                d.0.populate(input, length)?;
                let length = d.0.length();
                d.1 .0.populate(input, length)?;
                d.1 .1.populate(input, length)
            }
            Self::Seq(d) => {
                d.0.populate(input, length)?;
                let length = d.0.length();
                d.1.populate(input, length)
            }
            Self::Str(d) => d.populate(input, length),
            Self::Tuple(d) => d.iter_mut().try_for_each(|d| d.populate(input, length)),
            Self::U8(d) => d.populate(input, length),
            Self::U16(d) => d.populate(input, length),
            Self::U32(d) => d.populate(input, length),
            Self::U64(d) => d.populate(input, length),
            Self::U128(d) => d.populate(input, length),
            Self::Unpopulated => {
                *self = Self::Unspecified { length };
                Ok(())
            }
            Self::Unspecified { .. } => unreachable!(),
        }
    }
}

struct DecoderWrapper<'a, 'de> {
    decoder: &'a mut SerdeDecoder<'de>,
    input: &'a mut &'de [u8],
}

macro_rules! specify {
    ($self:ident, $variant:ident) => {{
        match &mut $self.decoder {
            // Check if it's already the correct decoder. This results in 1 branch in the hot path.
            SerdeDecoder::$variant(_) => (),
            _ => {
                // Either create the correct decoder if unspecified or diverge via panic/error.
                #[cold]
                #[rustfmt::skip]
                fn cold<'de>(decoder: &mut SerdeDecoder<'de>, input: &mut &'de [u8]) -> Result<()> {
                    let &mut SerdeDecoder::Unspecified { length } = decoder else {
                        type_changed!()
                    };
                    *decoder = SerdeDecoder::$variant(Default::default());
                    decoder.populate(input, length)
                }
                cold(&mut *$self.decoder, &mut *$self.input)?;
            }
        }
        #[rustfmt::skip]
        let SerdeDecoder::$variant(d) = &mut *$self.decoder else {
            // Safety: `cold` gets called when decoder isn't the correct decoder. `cold` either
            // errors or sets lazy to the correct decoder.
            unsafe { core::hint::unreachable_unchecked() };
        };
        d
    }};
}

macro_rules! impl_de {
    ($deserialize:ident, $visit:ident, $t:ty, $variant:ident) => {
        #[inline(always)]
        fn $deserialize<V>(mut self, v: V) -> Result<V::Value>
        where
            V: Visitor<'de>,
        {
            v.$visit(specify!(self, $variant).decode())
        }
    };
}

impl<'de> Deserializer<'de> for DecoderWrapper<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, _: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        err("deserialize_any is not supported")
    }

    // Use native decoders.
    impl_de!(deserialize_bool, visit_bool, bool, Bool);
    impl_de!(deserialize_f32, visit_f32, f32, F32);
    impl_de!(deserialize_u8, visit_u8, u8, U8);
    impl_de!(deserialize_u16, visit_u16, u16, U16);
    impl_de!(deserialize_u32, visit_u32, u32, U32);
    impl_de!(deserialize_u64, visit_u64, u64, U64);
    impl_de!(deserialize_u128, visit_u128, u128, U128);
    impl_de!(deserialize_str, visit_borrowed_str, &str, Str);

    // IntDecoder<unsigned> works on signed integers/f64 (but not chars).
    impl_de!(deserialize_i8, visit_i8, i8, U8);
    impl_de!(deserialize_i16, visit_i16, i16, U16);
    impl_de!(deserialize_i32, visit_i32, i32, U32);
    impl_de!(deserialize_i64, visit_i64, i64, U64);
    impl_de!(deserialize_i128, visit_i128, i128, U128);
    impl_de!(deserialize_f64, visit_f64, f64, U64);

    #[inline(always)]
    fn deserialize_char<V>(self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_char(char::from_u32(u32::deserialize(self)?).ok_or_else(|| error("invalid char"))?)
    }

    #[inline(always)]
    fn deserialize_string<V>(self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(v)
    }

    #[inline(always)]
    fn deserialize_bytes<V>(self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_byte_buf(v) // TODO avoid allocation.
    }

    #[inline(always)]
    fn deserialize_byte_buf<V>(self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_byte_buf(<Vec<u8>>::deserialize(self)?)
    }

    #[inline(always)]
    fn deserialize_option<V>(mut self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let (variant_decoder, decoders) = specify!(self, Enum);
        let variant_index = variant_decoder.decode();
        // Safety: populate guarantees `variant_decoder.max_variant_index() < decoders.len()`.
        let decoder = unsafe { decoders.get_unchecked_mut(variant_index as usize) };

        match variant_index {
            0 => v.visit_none(),
            1 => v.visit_some(DecoderWrapper {
                decoder,
                input: &mut *self.input,
            }),
            _ => err("invalid option"),
        }
    }

    #[inline(always)]
    fn deserialize_unit<V>(self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_unit()
    }

    #[inline(always)]
    fn deserialize_unit_struct<V>(self, _: &'static str, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_unit()
    }

    #[inline(always)]
    fn deserialize_newtype_struct<V>(self, _: &'static str, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(mut self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let (length_decoder, decoder) = specify!(self, Seq);
        let len = length_decoder.decode();

        struct Access<'a, 'de> {
            wrapper: DecoderWrapper<'a, 'de>,
            len: usize,
        }
        impl<'de> SeqAccess<'de> for Access<'_, 'de> {
            type Error = Error;

            #[inline(always)]
            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: DeserializeSeed<'de>,
            {
                guard_zst::<T::Value>(self.len)?;
                if self.len != 0 {
                    self.len -= 1;
                    Ok(Some(DeserializeSeed::deserialize(
                        seed,
                        DecoderWrapper {
                            decoder: &mut *self.wrapper.decoder,
                            input: &mut *self.wrapper.input,
                        },
                    )?))
                } else {
                    Ok(None)
                }
            }

            #[inline(always)]
            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }
        v.visit_seq(Access {
            wrapper: DecoderWrapper {
                decoder,
                input: self.input,
            },
            len,
        })
    }

    #[inline(always)]
    fn deserialize_tuple<V>(mut self, tuple_len: usize, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Fast path: avoid overhead of tuple for 1 element.
        if tuple_len == 1 {
            return v.visit_seq(Access {
                decoders: core::slice::from_mut(self.decoder),
                input: self.input,
                index: 0,
            });
        }

        // Copy of specify! macro that takes an additional tuple_len parameter to cold.
        match &mut self.decoder {
            SerdeDecoder::Tuple(_) => (),
            _ => {
                #[cold]
                fn cold<'de>(
                    decoder: &mut SerdeDecoder<'de>,
                    input: &mut &'de [u8],
                    tuple_len: usize,
                ) -> Result<()> {
                    let &mut SerdeDecoder::Unspecified { length } = decoder else {
                        type_changed!()
                    };
                    *decoder = SerdeDecoder::Tuple(default_box_slice(tuple_len));
                    decoder.populate(input, length)
                }
                cold(&mut *self.decoder, &mut *self.input, tuple_len)?;
            }
        }
        let SerdeDecoder::Tuple(decoders) = &mut *self.decoder else {
            // Safety: see specify! macro which this is based on.
            unsafe { core::hint::unreachable_unchecked() };
        };
        if decoders.len() != tuple_len {
            type_changed!() // Removes multiple bounds checks.
        }

        struct Access<'a, 'de> {
            decoders: &'a mut [SerdeDecoder<'de>],
            input: &'a mut &'de [u8],
            index: usize,
        }
        impl<'de> SeqAccess<'de> for Access<'_, 'de> {
            type Error = Error;

            #[inline(always)]
            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: DeserializeSeed<'de>,
            {
                if let Some(decoder) = self.decoders.get_mut(self.index) {
                    self.index += 1;
                    Ok(Some(DeserializeSeed::deserialize(
                        seed,
                        DecoderWrapper {
                            decoder,
                            input: &mut *self.input,
                        },
                    )?))
                } else {
                    Ok(None)
                }
            }

            #[inline(always)]
            fn size_hint(&self) -> Option<usize> {
                Some(self.decoders.len())
            }
        }

        v.visit_seq(Access {
            decoders,
            input: &mut *self.input,
            index: 0,
        })
    }

    #[inline(always)]
    fn deserialize_tuple_struct<V>(self, _: &'static str, len: usize, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(len, v)
    }

    fn deserialize_map<V>(mut self, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let (length_decoder, decoders) = specify!(self, Map);
        let len = length_decoder.decode();

        struct Access<'a, 'de> {
            decoders: &'a mut (SerdeDecoder<'de>, SerdeDecoder<'de>),
            input: &'a mut &'de [u8],
            len: usize,
            key_deserialized: bool,
        }

        impl<'de> MapAccess<'de> for Access<'_, 'de> {
            type Error = Error;

            #[inline(always)]
            fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
            where
                K: DeserializeSeed<'de>,
            {
                guard_zst::<K::Value>(self.len)?;
                if self.len != 0 {
                    self.len -= 1;
                    // Safety: Make sure next_value_seed is called at most once after each len decrement.
                    // We don't care if DeserializeSeed fails after this (not critical to safety).
                    self.key_deserialized = true;
                    Ok(Some(DeserializeSeed::deserialize(
                        seed,
                        DecoderWrapper {
                            decoder: &mut self.decoders.0,
                            input: &mut *self.input,
                        },
                    )?))
                } else {
                    Ok(None)
                }
            }

            #[inline(always)]
            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
            where
                V: DeserializeSeed<'de>,
            {
                // Safety: Make sure next_value_seed is called at most once after each len decrement
                // since only len values exist.
                assert!(
                    core::mem::take(&mut self.key_deserialized),
                    "next_value_seed before next_key_seed"
                );
                DeserializeSeed::deserialize(
                    seed,
                    DecoderWrapper {
                        decoder: &mut self.decoders.1,
                        input: &mut *self.input,
                    },
                )
            }
            // TODO implement next_entry_seed to avoid checking key_deserialized.

            #[inline(always)]
            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        v.visit_map(Access {
            decoders,
            input: self.input,
            len,
            key_deserialized: false, // No keys have been deserialized yet, so next_value_seed can't be called.
        })
    }

    #[inline(always)]
    fn deserialize_struct<V>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), v)
    }

    #[inline(always)]
    fn deserialize_enum<V>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        v.visit_enum(self)
    }

    fn deserialize_identifier<V>(self, _: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        err("deserialize_identifier is not supported")
    }

    fn deserialize_ignored_any<V>(self, _: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        err("deserialize_ignored_any is not supported")
    }

    #[inline(always)]
    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<'a, 'de> EnumAccess<'de> for DecoderWrapper<'a, 'de> {
    type Error = Error;
    type Variant = DecoderWrapper<'a, 'de>;

    #[inline(always)]
    fn variant_seed<V>(mut self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let (variant_decoder, decoders) = specify!(self, Enum);
        let variant_index = variant_decoder.decode();
        // Safety: populate guarantees `variant_decoder.max_variant_index() < decoders.len()`.
        let decoder = unsafe { decoders.get_unchecked_mut(variant_index as usize) };
        let variant_index = variant_index as u32;

        let val: Result<_> = seed.deserialize(variant_index.into_deserializer());
        Ok((
            val?,
            DecoderWrapper {
                decoder,
                input: &mut *self.input,
            },
        ))
    }
}

impl<'de> VariantAccess<'de> for DecoderWrapper<'_, 'de> {
    type Error = Error;

    #[inline(always)]
    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    #[inline(always)]
    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    #[inline(always)]
    fn tuple_variant<V>(self, len: usize, v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(len, v)
    }

    #[inline(always)]
    fn struct_variant<V>(self, fields: &'static [&'static str], v: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), v)
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use serde::de::MapAccess;
    use serde::Deserializer;

    #[test]
    fn deserialize() {
        macro_rules! test {
            ($v:expr, $t:ty) => {
                let v = $v;
                let ser = crate::serialize::<$t>(&v).unwrap();
                #[cfg(feature = "std")]
                println!("{:<24} {ser:?}", stringify!($t));
                assert_eq!(v, crate::deserialize::<$t>(&ser).unwrap());
            };
        }
        // Primitives
        test!(5, u8);
        test!(5, u16);
        test!(5, u32);
        test!(5, u64);
        test!(5, u128);
        test!(5, i8);
        test!(5, i16);
        test!(5, i32);
        test!(5, i64);
        test!(5, i128);
        test!(true, bool);
        test!('a', char);

        // Enums
        test!(Some(true), Option<bool>);
        test!(Ok(true), Result<bool, u32>);
        test!(vec![Ok(true), Err(2)], Vec<Result<bool, u32>>);
        test!(vec![Err(1), Ok(false)], Vec<Result<bool, u32>>);

        // Maps
        let mut map = BTreeMap::new();
        map.insert(1u8, 11u8);
        map.insert(2u8, 22u8);
        test!(map, BTreeMap<u8, u8>);

        // Sequences
        test!("abc".to_owned(), String);
        test!(vec![1u8, 2u8, 3u8], Vec<u8>);
        // Make sure signed integers are being packed properly (output should end in 85).
        test!(vec![0, -1, 0, -1, 0, -1, 0], Vec<i8>);
        test!(vec![0, -1, 0, -1, 0, -1, 0], Vec<i16>);
        test!(vec![0, -1, 0, -1, 0, -1, 0], Vec<i32>);
        test!(vec![0, -1, 0, -1, 0, -1, 0], Vec<i64>);
        test!(vec![0, -1, 0, -1, 0, -1, 0], Vec<i128>);
        // Make sure f32 sign_exp is grouped (output should end in 4x 63).
        test!(vec![1.0; 4], Vec<f32>);
        test!(
            vec!["abc".to_owned(), "def".to_owned(), "ghi".to_owned()],
            Vec<String>
        );

        // Tuples
        test!((1u8, 2u8, 3u8), (u8, u8, u8));
        test!([1u8, 2u8, 3u8], [u8; 3]);
        test!([], [u8; 0]);

        // Complex.
        test!(vec![(None, 3), (Some(4), 5)], Vec<(Option<u8>, u8)>);
    }

    #[test]
    #[should_panic = "next_value_seed before next_key_seed"]
    fn map_incorrect_len_values() {
        let mut map = BTreeMap::new();
        map.insert(1u8, 2u8);
        let input = crate::serialize(&map).unwrap();

        let w = super::DecoderWrapper {
            decoder: &mut super::SerdeDecoder::Unspecified { length: 1 },
            input: &mut input.as_slice(),
        };

        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ();
            fn expecting(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
                unreachable!()
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                assert_eq!(map.next_key::<u8>().unwrap().unwrap(), 1u8);
                assert_eq!(map.next_value::<u8>().unwrap(), 2u8);
                map.next_value::<u8>().unwrap();
                Ok(())
            }
        }
        w.deserialize_map(Visitor).unwrap();
    }
}
