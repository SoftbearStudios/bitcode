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
    pub fn serialize<T: Serialize + ?Sized>(t: &T) -> Result<Vec<u8>, Error> {
        let mut lazy = LazyEncoder::Unspecified {
            reserved: NonZeroUsize::new(1),
        };
        let mut index_alloc = 0;
        t.serialize(EncoderWrapper {
            lazy: &mut lazy,
            index_alloc: &mut index_alloc,
        })?;
        Ok(lazy.collect(index_alloc))
    }
}
pub use inner::serialize;

enum SpecifiedEncoder {
    Bool(BoolEncoder),
    Enum((VariantEncoder, Vec<LazyEncoder>)), // (variants, values)
    F32(F32Encoder),
    // Serialize needs separate signed integer encoders to be able to pack [0, -1, 0, -1, 0, -1].
    I8(IntEncoder<i8>),
    I16(IntEncoder<i16>),
    I32(IntEncoder<i32>),
    I64(IntEncoder<i64>),
    I128(IntEncoder<i128>),
    Map((LengthEncoder, Box<(LazyEncoder, LazyEncoder)>)), // (lengths, (keys, values))
    Seq((LengthEncoder, Box<LazyEncoder>)),                // (lengths, values)
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
    /// Analogous [`Buffer::collect`], but requires `index_alloc` from serialization.
    fn collect(&mut self, index_alloc: usize) -> Vec<u8> {
        // If we just wrote out the buffers in field order we wouldn't be able to deserialize them
        // since we might learn their types from serde in a different order.
        //
        // Consider the value: `[(vec![], 0u8), (vec![true], 1u8)]`
        // We don't know that the Vec contains bool until we've already deserialized 0u8.
        // Serde only tells us what is in sequences that aren't empty.
        //
        // Therefore, we have to reorder the buffers to match the order serde told us about them.
        let mut buffers = default_box_slice(index_alloc);
        self.reorder(&mut buffers);

        let mut bytes = vec![];
        for buffer in Vec::from(buffers).into_iter().flatten() {
            buffer.collect_into(&mut bytes);
        }
        bytes
    }

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
                });
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
    #[inline(always)]
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
            // Check if it's already the correct encoder. This results in 1 branch in the hot path.
            LazyEncoder::Specified { specified: SpecifiedEncoder::$variant(_), .. } => (),
            _ => {
                // Either create the correct encoder if unspecified or panic if we already have an
                // encoder since it must be a different type.
                #[cold]
                fn cold(
                    me: &mut LazyEncoder,
                    index_alloc: &mut usize,
                ) {
                    let &mut LazyEncoder::Unspecified { reserved } = me else {
                        type_changed!();
                    };
                    *me = LazyEncoder::Specified {
                        specified: SpecifiedEncoder::$variant(Default::default()),
                        index: std::mem::replace(index_alloc, *index_alloc + 1),
                    };
                    let LazyEncoder::Specified { specified, .. } = me else {
                        unreachable!();
                    };
                    if let Some(reserved) = reserved {
                        specified.reserve(reserved);
                    }
                }
                cold(lazy, &mut *$wrapper.index_alloc);
            }
        }
        let LazyEncoder::Specified { specified: SpecifiedEncoder::$variant(b), .. } = lazy else {
            // Safety: `cold` gets called when lazy isn't the correct encoder. `cold` either diverges
            // or sets lazy to the correct encoder.
            unsafe { std::hint::unreachable_unchecked() };
        };
        b
    }};
}

struct EncoderWrapper<'a> {
    lazy: &'a mut LazyEncoder,
    index_alloc: &'a mut usize,
}

impl<'a> EncoderWrapper<'a> {
    #[inline(always)]
    fn serialize_enum(self, variant_index: u32) -> Result<EncoderWrapper<'a>> {
        let variant_index = variant_index
            .try_into()
            .map_err(|_| error("enums with more than 256 variants are unsupported"))?;
        let b = specify!(self, Enum);
        b.0.encode(&variant_index);
        let lazy = get_mut_or_resize(&mut b.1, variant_index as usize);
        lazy.reserve_fast(1); // TODO use push instead.
        Ok(Self {
            lazy,
            index_alloc: self.index_alloc,
        })
    }
}

macro_rules! impl_ser {
    ($name:ident, $t:ty, $variant:ident) => {
        // TODO #[inline(always)] makes benchmark slower because collect_seq isn't inlined.
        fn $name(self, v: $t) -> Result<()> {
            specify!(self, $variant).encode(&v);
            Ok(())
        }
    };
}

impl<'a> Serializer for EncoderWrapper<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = SeqSerializer<'a>;
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

    #[inline(always)]
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

    #[inline(always)]
    fn serialize_unit(self) -> Result<Self::Ok> {
        Ok(())
    }

    #[inline(always)]
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        Ok(())
    }

    #[inline(always)]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok> {
        self.serialize_enum(variant_index)?;
        Ok(())
    }

    #[inline(always)]
    fn serialize_newtype_struct<T: ?Sized>(self, _name: &'static str, value: &T) -> Result<Self::Ok>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    #[inline(always)]
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
        let b = specify!(self, Seq);
        b.0.encode(&len);
        b.1.reserve_fast(len);
        Ok(SeqSerializer {
            lazy: &mut b.1,
            index_alloc: self.index_alloc,
            len,
        })
    }

    #[inline(always)]
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        // Fast path: avoid overhead of tuple for 1 element.
        if len == 1 {
            return Ok(TupleSerializer {
                encoders: std::slice::from_mut(self.lazy),
                index_alloc: self.index_alloc,
            });
        }

        // Copy of specify! macro that takes an additional len parameter to cold.
        let lazy = &mut *self.lazy;
        match lazy {
            LazyEncoder::Specified {
                specified: SpecifiedEncoder::Tuple(_),
                ..
            } => (),
            _ => {
                #[cold]
                fn cold(me: &mut LazyEncoder, len: usize) {
                    let &mut LazyEncoder::Unspecified { reserved } = me else {
                        type_changed!();
                    };
                    *me = LazyEncoder::Specified {
                        specified: SpecifiedEncoder::Tuple(default_box_slice(len)),
                        index: usize::MAX, // We never use index for SpecifiedEncoder::Tuple.
                    };
                    let LazyEncoder::Specified { specified, .. } = me else {
                        unreachable!();
                    };
                    if let Some(reserved) = reserved {
                        specified.reserve(reserved);
                    }
                }
                cold(lazy, len);
            }
        };
        let LazyEncoder::Specified {
            specified: SpecifiedEncoder::Tuple(encoders),
            ..
        } = lazy else {
            // Safety: see specify! macro which this is based on.
            unsafe { std::hint::unreachable_unchecked() };
        };
        if encoders.len() != len {
            type_changed!(); // Removes multiple bounds checks.
        }
        Ok(TupleSerializer {
            encoders,
            index_alloc: self.index_alloc,
        })
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
        let b = specify!(self, Map);
        b.0.encode(&len);
        b.1 .0.reserve_fast(len);
        b.1 .1.reserve_fast(len);
        Ok(MapSerializer {
            encoders: &mut b.1,
            index_alloc: self.index_alloc,
            len,
            key_serialized: false, // No keys have been serialized yet, so serialize_value can't be called.
        })
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

struct SeqSerializer<'a> {
    lazy: &'a mut LazyEncoder,
    index_alloc: &'a mut usize,
    len: usize,
}

impl SerializeSeq for SeqSerializer<'_> {
    ok_error_end!();
    #[inline(always)]
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        // Safety: Make sure safe code doesn't lie about len and cause UB since we've only reserved len elements.
        self.len = self.len.checked_sub(1).expect("length mismatch");
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
    ($tr:ty, $fun:ident $(, $key:ident)?) => {
        impl $tr for TupleSerializer<'_> {
            ok_error_end!();
            #[inline(always)]
            fn $fun<T: Serialize + ?Sized>(&mut self, $($key: &'static str,)? value: &T) -> Result<()> {
                let (lazy, remaining) = std::mem::take(&mut self.encoders)
                    .split_first_mut()
                    .expect("length mismatch");
                self.encoders = remaining;
                value.serialize(EncoderWrapper {
                    lazy,
                    index_alloc: &mut *self.index_alloc,
                })
            }

            $(
                fn skip_field(&mut self, $key: &'static str) -> Result<()> {
                    err("skip field is not supported")
                }
            )?
        }
    };
}
impl_tuple!(SerializeTuple, serialize_element);
impl_tuple!(SerializeTupleStruct, serialize_field);
impl_tuple!(SerializeTupleVariant, serialize_field);
impl_tuple!(SerializeStruct, serialize_field, _key);
impl_tuple!(SerializeStructVariant, serialize_field, _key);

struct MapSerializer<'a> {
    encoders: &'a mut (LazyEncoder, LazyEncoder), // (keys, values)
    index_alloc: &'a mut usize,
    len: usize,
    key_serialized: bool,
}

impl SerializeMap for MapSerializer<'_> {
    ok_error_end!();
    #[inline(always)]
    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<()>
    where
        T: Serialize,
    {
        // Safety: Make sure safe code doesn't lie about len and cause UB since we've only reserved len keys/values.
        self.len = self.len.checked_sub(1).expect("length mismatch");
        // Safety: Make sure serialize_value is called at most once after each serialize_key.
        self.key_serialized = true;
        key.serialize(EncoderWrapper {
            lazy: &mut self.encoders.0,
            index_alloc: &mut *self.index_alloc,
        })
    }

    #[inline(always)]
    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        // Safety: Make sure serialize_value is called at most once after each serialize_key.
        assert!(
            std::mem::take(&mut self.key_serialized),
            "serialize_value before serialize_key"
        );
        value.serialize(EncoderWrapper {
            lazy: &mut self.encoders.1,
            index_alloc: &mut *self.index_alloc,
        })
    }
    // TODO implement serialize_entry to avoid checking key_serialized.
}

#[cfg(test)]
mod tests {
    use serde::ser::{SerializeMap, SerializeSeq, SerializeTuple};
    use serde::{Serialize, Serializer};
    use std::num::NonZeroUsize;

    #[test]
    fn enum_256_variants() {
        enum Enum {
            A,
            B,
        }
        impl Serialize for Enum {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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

    #[test]
    #[should_panic(expected = "type changed")]
    fn test_type_changed() {
        struct BoolOrU8(bool);
        impl Serialize for BoolOrU8 {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if self.0 {
                    serializer.serialize_bool(false)
                } else {
                    serializer.serialize_u8(1)
                }
            }
        }
        let _ = crate::serialize(&vec![BoolOrU8(false), BoolOrU8(true)]);
    }

    #[test]
    #[should_panic(expected = "type changed")]
    fn test_tuple_len_changed() {
        struct TupleN(usize);
        impl Serialize for TupleN {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut tuple = serializer.serialize_tuple(self.0)?;
                (0..self.0).try_for_each(|_| tuple.serialize_element(&false))?;
                tuple.end()
            }
        }
        let _ = crate::serialize(&vec![TupleN(1), TupleN(2)]);
    }

    // Has to be a macro because it borrows something on the stack and returns it.
    macro_rules! new_wrapper {
        () => {
            super::EncoderWrapper {
                lazy: &mut super::LazyEncoder::Unspecified {
                    reserved: NonZeroUsize::new(1),
                },
                index_alloc: &mut 0,
            }
        };
    }

    #[test]
    fn seq_valid() {
        let w = new_wrapper!();
        let mut seq = w.serialize_seq(Some(1)).unwrap();
        let _ = seq.serialize_element(&0u8); // serialize_seq 1 == serialize 1.
    }

    #[test]
    #[should_panic = "length mismatch"]
    fn seq_incorrect_len() {
        let w = new_wrapper!();
        let mut seq = w.serialize_seq(Some(1)).unwrap();
        let _ = seq.serialize_element(&0u8); // serialize_seq 1 != serialize 2.
        let _ = seq.serialize_element(&0u8);
    }

    #[test]
    fn map_valid() {
        let w = new_wrapper!();
        let mut map = w.serialize_map(Some(1)).unwrap();
        let _ = map.serialize_key(&0u8); // serialize_map 1 == (key, value).
        let _ = map.serialize_value(&0u8);
    }

    #[test]
    #[should_panic = "length mismatch"]
    fn map_incorrect_len_keys() {
        let w = new_wrapper!();
        let mut map = w.serialize_map(Some(1)).unwrap();
        let _ = map.serialize_key(&0u8); // serialize_map 1 != (key, _) (key, _)
        let _ = map.serialize_key(&0u8);
    }

    #[test]
    #[should_panic = "serialize_value before serialize_key"]
    fn map_value_before_key() {
        let w = new_wrapper!();
        let mut map = w.serialize_map(Some(1)).unwrap();
        let _ = map.serialize_value(&0u8);
    }

    #[test]
    #[should_panic = "serialize_value before serialize_key"]
    fn map_incorrect_len_values() {
        let w = new_wrapper!();
        let mut map = w.serialize_map(Some(1)).unwrap();
        let _ = map.serialize_key(&0u8); // serialize_map 1 != (key, value) (_, value).
        let _ = map.serialize_value(&0u8);
        let _ = map.serialize_value(&0u8);
    }
}
