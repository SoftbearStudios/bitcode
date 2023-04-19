use crate::nightly::utf8_char_width;
use crate::read::Read;
use crate::{Error, Result, E};
use serde::de::{
    DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess, Visitor,
};
use serde::{Deserialize, Deserializer};

pub fn deserialize_internal<'a, T: Deserialize<'a>>(
    r: &mut (impl Read + Default),
    bytes: &[u8],
) -> Result<T> {
    r.start_read(bytes);

    // We take the reader and replace it if no error occurred.
    let mut d = BitcodeDeserializer {
        reader: std::mem::take(r),
    };
    let deserialize_result = T::deserialize(&mut d);

    // WordBuffer can read slightly more than the input without realizing it (for performance).
    let finish_result = d.reader.finish_read();
    if let Err(e) = &finish_result {
        if e.same(&E::Eof.e()) {
            return Err(E::Eof.e());
        }
    }

    let t = deserialize_result?;
    finish_result?;

    // No error occurred so we can replace the reader.
    *r = d.reader;
    Ok(t)
}

struct BitcodeDeserializer<R> {
    reader: R,
}

macro_rules! read_int {
    ($name:ident, $a:ty) => {
        fn $name(&mut self) -> Result<$a> {
            self.reader.read_bits(<$a>::BITS as usize).map(|v| v as $a)
        }
    };
}

impl<R: Read> BitcodeDeserializer<R> {
    read_int!(read_u8, u8);
    read_int!(read_u16, u16);
    read_int!(read_u32, u32);
    read_int!(read_u64, u64);

    fn read_bool(&mut self) -> Result<bool> {
        self.reader.read_bit()
    }

    fn read_len(&mut self) -> Result<usize> {
        let max_zeros = (usize::BITS - 1) as usize;
        let zero_bits = self
            .reader
            .read_zeros(max_zeros)
            .map_err(|e| e.map_invalid("length"))?;

        let integer_bits = zero_bits + 1;
        let rotated = self.reader.read_bits(integer_bits)?;

        // Rotate bits mod `integer_bits` instead of reversing since it's faster.
        // 0000bbb1 -> 00001bbb
        let v = (rotated as u64 >> 1) | (1 << (integer_bits - 1));

        // Gamma can't encode 0 so sub 1 (see serialize_len for more details).
        Ok((v - 1) as usize)
    }

    #[inline]
    fn read_len_and_bytes_inner(&mut self) -> Result<&[u8]> {
        let len = self.read_len()?;
        self.reader.read_bytes(len)
    }

    #[inline]
    fn read_len_and_bytes(&mut self) -> Result<&[u8]> {
        self.read_len_and_bytes_inner()
    }

    #[inline(never)] // Removing this makes bench_bitcode_deserialize 27% slower.
    fn read_len_and_byte_buf(&mut self) -> Result<Vec<u8>> {
        Ok(self.read_len_and_bytes_inner()?.to_owned())
    }

    fn read_variant_index(&mut self) -> Result<u32> {
        Ok(self
            .read_len()
            .map_err(|e| e.map_invalid("variant index"))? as u32)
    }
}

macro_rules! deserialize_int {
    ($name:ident, $visit:ident, $read:ident, $a:ty) => {
        fn $name<V>(self, visitor: V) -> Result<V::Value>
        where
            V: Visitor<'de>,
        {
            visitor.$visit(self.$read()? as $a)
        }
    };
}

impl<'de, R: Read> Deserializer<'de> for &mut BitcodeDeserializer<R> {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(E::NotSupported("deserialize_any").e())
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.read_bool()?)
    }

    deserialize_int!(deserialize_i8, visit_i8, read_u8, i8);
    deserialize_int!(deserialize_i16, visit_i16, read_u16, i16);
    deserialize_int!(deserialize_i32, visit_i32, read_u32, i32);
    deserialize_int!(deserialize_i64, visit_i64, read_u64, i64);
    deserialize_int!(deserialize_u8, visit_u8, read_u8, u8);
    deserialize_int!(deserialize_u16, visit_u16, read_u16, u16);
    deserialize_int!(deserialize_u32, visit_u32, read_u32, u32);
    deserialize_int!(deserialize_u64, visit_u64, read_u64, u64);

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f32(f32::from_bits(self.read_u32()?))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f64(f64::from_bits(self.read_u64()?))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        buf[0] = self.read_u8()?;

        let len = utf8_char_width(buf[0]);
        if len > 1 {
            let bits = self.reader.read_bits((len - 1) * u8::BITS as usize)?;
            buf[1..len].copy_from_slice(&bits.to_le_bytes()[0..len - 1]);
        }

        let s = std::str::from_utf8(&buf[..len]).map_err(|_| E::Invalid("char").e())?;
        debug_assert_eq!(s.as_bytes().len(), len);
        debug_assert_eq!(s.chars().count(), 1);
        visitor.visit_char(s.chars().next().unwrap())
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let bytes = self.read_len_and_bytes()?;
        visitor.visit_str(std::str::from_utf8(bytes).map_err(|_| E::Invalid("utf8").e())?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let bytes = self.read_len_and_byte_buf()?;
        visitor.visit_string(String::from_utf8(bytes).map_err(|_| E::Invalid("utf8").e())?)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bytes(self.read_len_and_bytes()?)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_byte_buf(self.read_len_and_byte_buf()?)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if self.read_bool()? {
            visitor.visit_some(self)
        } else {
            visitor.visit_none()
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let len = self.read_len()?;
        self.deserialize_tuple(len, visitor)
    }

    // based on https://github.com/bincode-org/bincode/blob/c44b5e364e7084cdbabf9f94b63a3c7f32b8fb68/src/de/mod.rs#L293-L330
    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        struct Access<'a, R> {
            deserializer: &'a mut BitcodeDeserializer<R>,
            len: usize,
        }

        impl<'de, R: Read> SeqAccess<'de> for Access<'_, R> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: DeserializeSeed<'de>,
            {
                if std::mem::size_of::<T>() == 0 {
                    guard_zst(self.len)?;
                }
                if self.len > 0 {
                    self.len -= 1;
                    let value = DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        visitor.visit_seq(Access {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    // based on https://github.com/bincode-org/bincode/blob/c44b5e364e7084cdbabf9f94b63a3c7f32b8fb68/src/de/mod.rs#L353-L400
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        struct Access<'a, R: Read> {
            deserializer: &'a mut BitcodeDeserializer<R>,
            len: usize,
        }

        impl<'de, R: Read> MapAccess<'de> for Access<'_, R> {
            type Error = Error;

            fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
            where
                K: DeserializeSeed<'de>,
            {
                if std::mem::size_of::<K>() == 0 {
                    guard_zst(self.len)?;
                }
                if self.len > 0 {
                    self.len -= 1;
                    let key = DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(key))
                } else {
                    Ok(None)
                }
            }

            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
            where
                V: DeserializeSeed<'de>,
            {
                let value = DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                Ok(value)
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        let len = self.read_len()?;
        visitor.visit_map(Access {
            deserializer: self,
            len,
        })
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    // based on https://github.com/bincode-org/bincode/blob/c44b5e364e7084cdbabf9f94b63a3c7f32b8fb68/src/de/mod.rs#L263-L291
    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        impl<'a, 'de, R: Read> EnumAccess<'de> for &'a mut BitcodeDeserializer<R> {
            type Error = Error;
            type Variant = &'a mut BitcodeDeserializer<R>;

            fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
            where
                V: DeserializeSeed<'de>,
            {
                let idx = self.read_variant_index()?;
                let val: Result<_> = seed.deserialize(idx.into_deserializer());
                Ok((val?, self))
            }
        }

        visitor.visit_enum(self)
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(E::NotSupported("deserialize_identifier").e())
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(E::NotSupported("deserialize_ignored_any").e())
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

// Guards against Vec<()> with huge len taking forever.
fn guard_zst(len: usize) -> Result<()> {
    if len > 1 << 16 {
        Err(E::Invalid("too many zst").e())
    } else {
        Ok(())
    }
}

// based on https://github.com/bincode-org/bincode/blob/c44b5e364e7084cdbabf9f94b63a3c7f32b8fb68/src/de/mod.rs#L461-L492
impl<'de, R: Read> VariantAccess<'de> for &mut BitcodeDeserializer<R> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        DeserializeSeed::deserialize(seed, self)
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Deserializer::deserialize_tuple(self, fields.len(), visitor)
    }
}
