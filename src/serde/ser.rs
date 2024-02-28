use crate::bool::BoolEncoder;
use crate::coder::{Buffer, Encoder, Result};
use crate::error::{err, error, Error};
use crate::f32::F32Encoder;
use crate::int::IntEncoder;
use crate::length::LengthEncoder;
use crate::serde::variant::VariantEncoder;
use crate::serde::{default_box_slice, get_mut_or_resize, type_changed};
use crate::str::StrEncoder;
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Serialize, Serializer};
use std::num::NonZeroUsize;

// Redefine Result from crate::coder::Result to std::result::Result since the former isn't public.
mod inner {
    use super::*;
    use std::result::Result;

    /// Serializes a `T:` [`Serialize`] into a [`Vec<u8>`].
    ///
    /// **Warning:** The format is incompatible with [`decode`][`crate::decode`] and subject to
    /// change between major versions.
    #[cfg_attr(doc, doc(cfg(feature = "serde")))]
    pub fn serialize<T: Serialize + ?Sized>(t: &T) -> Result<Vec<u8>, Error> {
        let mut lazy = LazyEncoder::Unspecified {
            reserved: NonZeroUsize::new(1),
        };
        let mut index_alloc = 0;
        t.serialize(EncoderWrapper {
            lazy: &mut lazy,
            index_alloc: &mut index_alloc,
        })?;

        // If we just wrote out the buffers in field order we wouldn't be able to deserialize them
        // since we might learn their types from serde in a different order.
        //
        // Consider the value: `[(vec![], 0u8), (vec![true], 1u8)]`
        // We don't know that the Vec contains bool until we've already deserialized 0u8.
        // Serde only tells us what is in sequences that aren't empty.
        //
        // Therefore, we have to reorder the buffers to match the order serde told us about them.
        let mut buffers = default_box_slice(index_alloc);
        lazy.reorder(&mut buffers);

        let mut bytes = vec![];
        for buffer in Vec::from(buffers).into_iter().flatten() {
            buffer.collect_into(&mut bytes);
        }
        Ok(bytes)
    }
}
pub use inner::serialize;

#[derive(Debug)]
enum SpecifiedEncoder {
    Bool(BoolEncoder),
    Enum(Box<(VariantEncoder, Vec<LazyEncoder>)>), // (variants, values) TODO only 1 allocation?
    F32(F32Encoder),
    // Serialize needs separate signed integer encoders to be able to pack [0, -1, 0, -1, 0, -1].
    I8(IntEncoder<i8>),
    I16(IntEncoder<i16>),
    I32(IntEncoder<i32>),
    I64(IntEncoder<i64>),
    I128(IntEncoder<i128>),
    Map(Box<(LengthEncoder, (LazyEncoder, LazyEncoder))>), // (lengths, (keys, values))
    Seq(Box<(LengthEncoder, LazyEncoder)>),                // (lengths, values)
    Str(StrEncoder),
    Tuple(Box<[LazyEncoder]>), // [field0, field1, ..]
    U8(IntEncoder<u8>),
    U16(IntEncoder<u16>),
    U32(IntEncoder<u32>),
    U64(IntEncoder<u64>),
    U128(IntEncoder<u128>),
}

impl SpecifiedEncoder {
    fn reserve(&mut self, additional: NonZeroUsize) {
        match self {
            Self::Bool(v) => v.reserve(additional),
            Self::Enum(v) => {
                v.0.reserve(additional);
                // We don't know the variants of the enums, so we can't reserve more.
            }
            Self::F32(v) => v.reserve(additional),
            Self::I8(v) => v.reserve(additional),
            Self::I16(v) => v.reserve(additional),
            Self::I32(v) => v.reserve(additional),
            Self::I64(v) => v.reserve(additional),
            Self::I128(v) => v.reserve(additional),
            Self::Map(v) => {
                v.0.reserve(additional);
                // We don't know the lengths of the maps, so we can't reserve more.
            }
            Self::Seq(v) => {
                v.0.reserve(additional);
                // We don't know the lengths of the sequences, so we can't reserve more.
            }
            Self::Str(v) => {
                v.reserve(additional);
            }
            Self::Tuple(v) => v.iter_mut().for_each(|v| v.reserve_fast(additional.get())),
            Self::U8(v) => v.reserve(additional),
            Self::U16(v) => v.reserve(additional),
            Self::U32(v) => v.reserve(additional),
            Self::U64(v) => v.reserve(additional),
            Self::U128(v) => v.reserve(additional),
        }
    }
}

#[derive(Debug)]
enum LazyEncoder {
    Unspecified {
        reserved: Option<NonZeroUsize>,
    },
    Specified {
        specified: SpecifiedEncoder,
        index: usize,
    },
}

impl Default for LazyEncoder {
    fn default() -> Self {
        Self::Unspecified { reserved: None }
    }
}

impl LazyEncoder {
    fn reorder<'a>(&'a mut self, buffers: &mut [Option<&'a mut dyn Buffer>]) {
        match self {
            Self::Specified { specified, index } => {
                buffers[*index] = Some(match specified {
                    SpecifiedEncoder::Bool(v) => v,
                    SpecifiedEncoder::Enum(v) => {
                        v.1.iter_mut().for_each(|v| v.reorder(buffers));
                        &mut v.0
                    }
                    SpecifiedEncoder::F32(v) => v,
                    SpecifiedEncoder::I8(v) => v,
                    SpecifiedEncoder::I16(v) => v,
                    SpecifiedEncoder::I32(v) => v,
                    SpecifiedEncoder::I64(v) => v,
                    SpecifiedEncoder::I128(v) => v,
                    SpecifiedEncoder::Map(v) => {
                        v.1 .0.reorder(buffers);
                        v.1 .1.reorder(buffers);
                        &mut v.0
                    }
                    SpecifiedEncoder::Seq(v) => {
                        v.1.reorder(buffers);
                        &mut v.0
                    }
                    SpecifiedEncoder::Str(v) => v,
                    SpecifiedEncoder::Tuple(v) => {
                        v.iter_mut().for_each(|v| v.reorder(buffers));
                        return; // Has no buffer.
                    }
                    SpecifiedEncoder::U8(v) => v,
                    SpecifiedEncoder::U16(v) => v,
                    SpecifiedEncoder::U32(v) => v,
                    SpecifiedEncoder::U64(v) => v,
                    SpecifiedEncoder::U128(v) => v,
                })
            }
            Self::Unspecified { .. } => (),
        }
    }

    /// OLD COMMENT:
    /// Only reserves if the type is unspecified to save time. Speeds up large 1 time collections
    /// without slowing down many small collections too much. Takes a `usize` instead of a
    /// [`NonZeroUsize`] to avoid branching on len.
    ///
    /// Can't be reserve_fast anymore with push_within_capacity.
    fn reserve_fast(&mut self, len: usize) {
        match self {
            Self::Specified { specified, .. } => {
                if let Some(len) = NonZeroUsize::new(len) {
                    specified.reserve(len);
                }
            }
            Self::Unspecified { reserved } => *reserved = NonZeroUsize::new(len),
        }
    }
}

macro_rules! specify {
    ($wrapper:ident, $variant:ident) => {{
        let lazy = &mut *$wrapper.lazy;
        match lazy {
            LazyEncoder::Unspecified { reserved } => {
                let reserved = *reserved;
                #[cold]
                fn cold<'a>(
                    me: &'a mut LazyEncoder,
                    index_alloc: &mut usize,
                    reserved: Option<NonZeroUsize>,
                ) -> &'a mut SpecifiedEncoder {
                    let mut specified = SpecifiedEncoder::$variant(Default::default());
                    if let Some(reserved) = reserved {
                        specified.reserve(reserved);
                    }
                    *me = LazyEncoder::Specified {
                        specified,
                        index: std::mem::replace(index_alloc, *index_alloc + 1),
                    };
                    // TODO might be slower to put in cold fn.
                    if let LazyEncoder::Specified { specified, .. } = me {
                        specified
                    } else {
                        unreachable!();
                    }
                }
                cold(lazy, &mut *$wrapper.index_alloc, reserved)
            }
            LazyEncoder::Specified { specified, .. } => specified,
        }
    }};
}

struct EncoderWrapper<'a> {
    lazy: &'a mut LazyEncoder,
    index_alloc: &'a mut usize,
}

impl<'a> EncoderWrapper<'a> {
    fn serialize_enum(self, variant_index: u32) -> Result<EncoderWrapper<'a>> {
        let variant_index = variant_index
            .try_into()
            .map_err(|_| error("enums with more than 256 variants are unsupported"))?;
        match specify!(self, Enum) {
            SpecifiedEncoder::Enum(b) => {
                b.0.encode(&variant_index);
                let lazy = get_mut_or_resize(&mut b.1, variant_index as usize);
                lazy.reserve_fast(1); // TODO use push instead.
                Ok(Self {
                    lazy,
                    index_alloc: self.index_alloc,
                })
            }
            _ => type_changed(),
        }
    }
}

macro_rules! impl_ser {
    ($name:ident, $t:ty, $variant:ident) => {
        fn $name(self, v: $t) -> Result<()> {
            match specify!(self, $variant) {
                SpecifiedEncoder::$variant(b) => b.encode(&v),
                _ => return type_changed(),
            }
            Ok(())
        }
    };
}

impl<'a> Serializer for EncoderWrapper<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = EncoderWrapper<'a>;
    type SerializeTuple = TupleSerializer<'a>;
    type SerializeTupleStruct = TupleSerializer<'a>;
    type SerializeTupleVariant = TupleSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = TupleSerializer<'a>;
    type SerializeStructVariant = TupleSerializer<'a>;

    // Use native encoders.
    impl_ser!(serialize_bool, bool, Bool);
    impl_ser!(serialize_f32, f32, F32);
    impl_ser!(serialize_i8, i8, I8);
    impl_ser!(serialize_i16, i16, I16);
    impl_ser!(serialize_i32, i32, I32);
    impl_ser!(serialize_i64, i64, I64);
    impl_ser!(serialize_i128, i128, I128);
    impl_ser!(serialize_str, &str, Str);
    impl_ser!(serialize_u8, u8, U8);
    impl_ser!(serialize_u16, u16, U16);
    impl_ser!(serialize_u32, u32, U32);
    impl_ser!(serialize_u64, u64, U64);
    impl_ser!(serialize_u128, u128, U128);

    // IntEncoder works on f64/char.
    impl_ser!(serialize_f64, f64, U64);
    impl_ser!(serialize_char, char, U32);

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok> {
        v.serialize(self)
    }

    #[inline(always)]
    fn serialize_none(self) -> Result<Self::Ok> {
        self.serialize_enum(0)?;
        Ok(())
    }

    #[inline(always)]
    fn serialize_some<T: ?Sized>(self, v: &T) -> Result<Self::Ok>
    where
        T: Serialize,
    {
        v.serialize(self.serialize_enum(1)?)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok> {
        self.serialize_enum(variant_index)?;
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized>(self, _name: &'static str, value: &T) -> Result<Self::Ok>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok>
    where
        T: Serialize,
    {
        value.serialize(self.serialize_enum(variant_index)?)
    }

    #[inline(always)]
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let len = len.expect("sequence must have len");
        match specify!(self, Seq) {
            SpecifiedEncoder::Seq(b) => {
                b.0.encode(&len);
                b.1.reserve_fast(len);
                Ok(Self {
                    lazy: &mut b.1,
                    index_alloc: self.index_alloc,
                })
            }
            _ => type_changed(),
        }
    }

    #[inline(always)]
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        let lazy = &mut *self.lazy;
        let specified = match lazy {
            &mut LazyEncoder::Unspecified { reserved } => {
                #[cold]
                fn cold(
                    me: &mut LazyEncoder,
                    reserved: Option<NonZeroUsize>,
                    len: usize,
                ) -> &mut SpecifiedEncoder {
                    let mut specified = SpecifiedEncoder::Tuple(default_box_slice(len));
                    if let Some(reserved) = reserved {
                        specified.reserve(reserved);
                    }
                    *me = LazyEncoder::Specified {
                        specified,
                        index: usize::MAX, // We never use this.
                    };
                    // TODO might be slower to put in cold fn.
                    let LazyEncoder::Specified { specified: encoder, .. } = me else {
                        unreachable!();
                    };
                    encoder
                }
                cold(lazy, reserved, len)
            }
            LazyEncoder::Specified { specified, .. } => specified,
        };
        match specified {
            SpecifiedEncoder::Tuple(encoders) => {
                assert_eq!(encoders.len(), len); // Removes multiple bounds checks.
                Ok(TupleSerializer {
                    encoders,
                    index_alloc: self.index_alloc,
                })
            }
            _ => type_changed(),
        }
    }

    #[inline(always)]
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_tuple(len)
    }

    #[inline(always)]
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_enum(variant_index)?.serialize_tuple(len)
    }

    #[inline(always)]
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        let len = len.expect("sequence must have len");
        match specify!(self, Map) {
            SpecifiedEncoder::Map(b) => {
                b.0.encode(&len);
                b.1 .0.reserve_fast(len);
                b.1 .1.reserve_fast(len);
                Ok(MapSerializer {
                    encoders: &mut b.1,
                    index_alloc: self.index_alloc,
                })
            }
            _ => type_changed(),
        }
    }

    #[inline(always)]
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_tuple(len)
    }

    #[inline(always)]
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_enum(variant_index)?.serialize_tuple(len)
    }

    #[inline(always)]
    fn is_human_readable(&self) -> bool {
        false
    }
}

macro_rules! ok_error_end {
    () => {
        type Ok = ();
        type Error = Error;
        fn end(self) -> Result<Self::Ok> {
            Ok(())
        }
    };
}

impl SerializeSeq for EncoderWrapper<'_> {
    ok_error_end!();
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(EncoderWrapper {
            lazy: &mut *self.lazy,
            index_alloc: &mut *self.index_alloc,
        })
    }
}

struct TupleSerializer<'a> {
    encoders: &'a mut [LazyEncoder], // [field0, field1, ..]
    index_alloc: &'a mut usize,
}

macro_rules! impl_tuple {
    ($tr:ty, $fun:ident) => {
        impl $tr for TupleSerializer<'_> {
            ok_error_end!();
            fn $fun<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
                let (lazy, remaining) = std::mem::take(&mut self.encoders)
                    .split_first_mut()
                    .expect("length mismatch");
                self.encoders = remaining;
                value.serialize(EncoderWrapper {
                    lazy,
                    index_alloc: &mut *self.index_alloc,
                })
            }
        }
    };
}
impl_tuple!(SerializeTuple, serialize_element);
impl_tuple!(SerializeTupleStruct, serialize_field);
impl_tuple!(SerializeTupleVariant, serialize_field);

macro_rules! impl_struct {
    ($tr:ty) => {
        impl $tr for TupleSerializer<'_> {
            ok_error_end!();
            fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, value: &T) -> Result<()>
            where
                T: Serialize,
            {
                let (lazy, remaining) = std::mem::take(&mut self.encoders)
                    .split_first_mut()
                    .expect("length mismatch");
                self.encoders = remaining;
                value.serialize(EncoderWrapper {
                    lazy,
                    index_alloc: &mut *self.index_alloc,
                })
            }

            fn skip_field(&mut self, _key: &'static str) -> Result<()> {
                err("skip field is not supported")
            }
        }
    };
}
impl_struct!(SerializeStruct);
impl_struct!(SerializeStructVariant);

struct MapSerializer<'a> {
    encoders: &'a mut (LazyEncoder, LazyEncoder), // (keys, values)
    index_alloc: &'a mut usize,
}

impl SerializeMap for MapSerializer<'_> {
    ok_error_end!();
    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<()>
    where
        T: Serialize,
    {
        key.serialize(EncoderWrapper {
            lazy: &mut self.encoders.0,
            index_alloc: &mut *self.index_alloc,
        })
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(EncoderWrapper {
            lazy: &mut self.encoders.1,
            index_alloc: &mut *self.index_alloc,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn enum_256_variants() {
        enum Enum {
            A,
            B,
        }
        impl serde::Serialize for Enum {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let variant_index = match self {
                    Self::A => 255,
                    Self::B => 256,
                };
                serializer.serialize_unit_variant("", variant_index, "")
            }
        }
        assert!(crate::serialize(&Enum::A).is_ok());
        assert!(crate::serialize(&Enum::B).is_err());
    }
}
