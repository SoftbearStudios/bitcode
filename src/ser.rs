use crate::nightly::ilog2;
use crate::{Error, Result, E};
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Serialize, Serializer};

pub(crate) mod write;
use write::{Write, WriteWith};

pub(crate) fn serialize_with<T: WriteWith>(t: &(impl Serialize + ?Sized)) -> Result<Vec<u8>> {
    Ok(serialize_into(t, T::default())?.into_inner())
}

pub(crate) fn serialize_into<W: Write>(t: &(impl Serialize + ?Sized), w: W) -> Result<W> {
    let mut s = BitcodeSerializer { data: w };
    t.serialize(&mut s)?;
    Ok(s.data)
}

#[derive(Default)]
struct BitcodeSerializer<W> {
    data: W,
}

impl<W: Write> BitcodeSerializer<W> {
    fn serialize_len(&mut self, len: usize) -> Result<()> {
        // https://en.wikipedia.org/wiki/Elias_gamma_coding
        // Gamma can't encode 0 so add 1. We don't support usize::MAX because it would add more code
        // and it's only useful for ZST.
        let v = len
            .checked_add(1)
            .ok_or(E::NotSupported("len must be < usize::MAX").e())?;

        let zeros = ilog2(v) as usize;
        let bit_count = zeros * 2 + 1;

        if bit_count <= 64 {
            let bits = (v as u64).reverse_bits() >> (u64::BITS as usize - bit_count);
            self.data.write_bits(bits, bit_count);
        } else {
            #[cold]
            fn slow(me: &mut BitcodeSerializer<impl Write>, v: usize) {
                let zeros = ilog2(v) as usize;
                me.data.write_bits(0, zeros);

                let integer_bits = zeros + 1;
                let lz = usize::BITS as usize - integer_bits;

                let bits = (v.reverse_bits() >> lz) as u64;
                me.data.write_bits(bits, integer_bits);
            }

            slow(self, v);
        }
        Ok(())
    }

    fn serialize_variant_index(&mut self, variant_index: u32) -> Result<()> {
        self.serialize_len(variant_index as usize)
    }
}

macro_rules! serialize_int {
    ($name:ident, $a:ty, $b:ty) => {
        fn $name(self, v: $a) -> Result<Self::Ok> {
            self.data.write_bits((v as $b).into(), <$b>::BITS as usize);
            Ok(())
        }
    };
}

impl<W: Write> Serializer for &mut BitcodeSerializer<W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok> {
        self.data.write_bit(v);
        Ok(())
    }

    serialize_int!(serialize_i8, i8, u8);
    serialize_int!(serialize_i16, i16, u16);
    serialize_int!(serialize_i32, i32, u32);
    serialize_int!(serialize_i64, i64, u64);
    serialize_int!(serialize_u8, u8, u8);
    serialize_int!(serialize_u16, u16, u16);
    serialize_int!(serialize_u32, u32, u32);
    serialize_int!(serialize_u64, u64, u64);

    fn serialize_char(self, v: char) -> Result<Self::Ok> {
        let mut buf = [0; 4];
        let string = v.encode_utf8(&mut buf);
        self.data.write_bytes(string.as_bytes());
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok> {
        self.serialize_u32(v.to_bits())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok> {
        self.serialize_u64(v.to_bits())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok> {
        self.serialize_bytes(v.as_bytes())
    }

    #[inline(never)] // Removing this makes bench_bitcode_serialize 7% slower.
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok> {
        if v.len() > isize::MAX as usize / u8::BITS as usize {
            return Err(E::NotSupported("bytes.len() must be < isize::MAX / u8::BITS").e());
        }
        self.serialize_len(v.len())?;
        self.data.write_bytes(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        self.serialize_bool(false)
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok>
    where
        T: Serialize,
    {
        self.serialize_bool(true)?;
        value.serialize(self)
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
        self.serialize_variant_index(variant_index)
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
        self.serialize_variant_index(variant_index)?;
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let len = len.expect("sequence must have len");
        self.serialize_len(len)?;
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_variant_index(variant_index)?;
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        let len = len.expect("sequence must have len");
        self.serialize_len(len)?;
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_variant_index(variant_index)?;
        Ok(self)
    }

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

impl<W: Write> SerializeSeq for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }
}

impl<W: Write> SerializeTuple for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }
}

impl<W: Write> SerializeTupleStruct for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }
}

impl<W: Write> SerializeTupleVariant for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }
}

impl<W: Write> SerializeMap for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<()>
    where
        T: Serialize,
    {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }
}

impl<W: Write> SerializeStruct for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _key: &'static str) -> Result<()> {
        Err(E::NotSupported("skip_field").e())
    }
}

impl<W: Write> SerializeStructVariant for &mut BitcodeSerializer<W> {
    ok_error_end!();
    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _key: &'static str) -> Result<()> {
        Err(E::NotSupported("skip_field").e())
    }
}
