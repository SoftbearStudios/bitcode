use crate::code::{Decode, Encode};
use crate::encoding::{Encoding, Fixed, Gamma};
use crate::guard::guard_len;
use crate::nightly::utf8_char_width;
use crate::read::Read;
use crate::write::Write;
use crate::{Result, E};
use std::hash::Hash;
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::num::*;

impl Encode for bool {
    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        writer.write_bit(*self);
        Ok(())
    }
}

impl Decode for bool {
    fn decode(_: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        reader.read_bit()
    }
}

macro_rules! impl_uints {
    ($($int: ty),*) => {
        $(
            impl Encode for $int {
                fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    encoding.write_word(writer, (*self).into(), <$int>::BITS as usize);
                    Ok(())
                }
            }

            impl Decode for $int {
                fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                    Ok(encoding.read_word(reader, <$int>::BITS as usize)? as $int)
                }
            }
        )*
    }
}

macro_rules! impl_ints {
    ($($int: ty => $uint: ty),*) => {
        $(
            impl Encode for $int {
                fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    let word = if encoding.zigzag() {
                        zigzag::ZigZagEncode::zigzag_encode(*self).into()
                    } else {
                        (*self as $uint).into()
                    };
                    encoding.write_word(writer, word, <$int>::BITS as usize);
                    Ok(())
                }
            }

            impl Decode for $int {
                fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                    let word = encoding.read_word(reader, <$int>::BITS as usize)?;
                    let sint = if encoding.zigzag() {
                        zigzag::ZigZagDecode::zigzag_decode(word as $uint)
                    } else {
                        word as $int
                    };
                    Ok(sint)
                }
            }
        )*
    }
}

impl_uints!(u8, u16, u32, u64);
impl_ints!(i8 => u8, i16 => u16, i32 => u32, i64 => u64);

macro_rules! impl_try_int {
    ($a:ty, $b:ty) => {
        impl Encode for $a {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                (*self as $b).encode(encoding, writer)
            }
        }

        impl Decode for $a {
            #[inline] // TODO is required?.
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                <$b>::decode(encoding, reader)?
                    .try_into()
                    .map_err(|_| E::Invalid(stringify!($a)).e())
            }
        }
    };
}

impl_try_int!(usize, u64);
impl_try_int!(isize, i64);

macro_rules! impl_float {
    ($a:ty, $write:ident, $read:ident) => {
        impl Encode for $a {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                encoding.$write(writer, *self);
                Ok(())
            }
        }

        impl Decode for $a {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                encoding.$read(reader)
            }
        }
    };
}

impl_float!(f32, write_f32, read_f32);
impl_float!(f64, write_f64, read_f64);

// Subtracts 1 in encode and adds one in decode (so gamma is smaller).
macro_rules! impl_non_zero {
    ($($a:ty),*) => {
        $(
            impl Encode for $a {
                fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    (self.get() - 1).encode(Fixed, writer)
                }
            }

            impl Decode for $a {
                fn decode(_: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                    let v = Decode::decode(Fixed, reader)?;
                    let _ = Self::new(v); // Type inference.
                    Self::new(v.wrapping_add(1)).ok_or_else(|| E::Invalid("non zero").e())
                }
            }
        )*
    };
}

impl_non_zero!(NonZeroU8, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroUsize);
impl_non_zero!(NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize);

impl Encode for char {
    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        let mut buf = [0; 4];
        let n = self.encode_utf8(&mut buf).len();
        writer.write_bits(u32::from_le_bytes(buf) as u64, n * u8::BITS as usize);
        Ok(())
    }
}

impl Decode for char {
    fn decode(_: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        let first = u8::decode(Fixed, reader)?;
        let len = utf8_char_width(first);

        let bytes = if len > 1 {
            let remaining = reader.read_bits((len - 1) * u8::BITS as usize)?;
            first as u32 | (remaining as u32) << u8::BITS
        } else {
            first as u32
        }
        .to_le_bytes();

        let s = std::str::from_utf8(&bytes[..len]).map_err(|_| E::Invalid("char").e())?;
        debug_assert_eq!(s.chars().count(), 1);
        Ok(s.chars().next().unwrap())
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        if let Some(t) = self {
            fn encode_some<T: Encode>(
                t: &T,
                encoding: impl Encoding,
                writer: &mut impl Write,
            ) -> Result<()> {
                writer.write_bit(true);
                t.encode(encoding, writer)
            }
            encode_some(t, encoding, writer)
        } else {
            writer.write_bit(false);
            Ok(())
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        Ok(if reader.read_bit()? {
            Some(Decode::decode(encoding, reader)?)
        } else {
            None
        })
    }
}

macro_rules! impl_either {
    ($typ: path, $a: ident, $b:ident, $is_b: ident $(,$($generic: ident);*)*) => {
        impl $(<$($generic: Encode),*>)* Encode for $typ {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                match self {
                    Self::$a(a) => {
                        debug_assert!(!self.$is_b());
                        writer.write_bit(false);
                        a.encode(encoding, writer)
                    },
                    Self::$b(b) => {
                        debug_assert!(self.$is_b());
                        writer.write_bit(true);
                        b.encode(encoding, writer)
                    },
                }
            }
        }

        impl $(<$($generic: Decode),*>)* Decode for $typ {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(if reader.read_bit()? {
                    Self::$b(Decode::decode(encoding, reader)?)
                } else {
                    Self::$a(Decode::decode(encoding, reader)?)
                })
            }
        }
    }
}

impl_either!(std::result::Result<T, E>, Ok, Err, is_err, T ; E);

macro_rules! impl_wrapper {
    ($(::$ptr: ident)*) => {
        impl<T: Encode> Encode for $(::$ptr)*<T> {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                T::encode(&self.0, encoding, writer)
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<T> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(Self(T::decode(encoding, reader)?))
            }
        }
    }
}

impl_wrapper!(::std::num::Wrapping);
impl_wrapper!(::std::cmp::Reverse);

macro_rules! impl_smart_ptr {
    ($(::$ptr: ident)*) => {
        impl<T: Encode + ?Sized> Encode for $(::$ptr)*<T> {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                T::encode(self, encoding, writer)
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<T> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(T::decode(encoding, reader)?.into())
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<[T]> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(Vec::<T>::decode(encoding, reader)?.into())
            }
        }

        impl Decode for $(::$ptr)*<str> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(String::decode(encoding, reader)?.into())
            }
        }
    }
}

impl_smart_ptr!(::std::boxed::Box);
impl_smart_ptr!(::std::rc::Rc);
impl_smart_ptr!(::std::sync::Arc);

impl<const N: usize, T: Encode> Encode for [T; N] {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        for t in self.iter() {
            t.encode(encoding, writer)?
        }
        Ok(())
    }
}

impl<const N: usize, T: Decode> Decode for [T; N] {
    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        // TODO find a safe way to decode an array without allocating.
        let v: Result<Vec<_>> = (0..N).map(|_| T::decode(encoding, reader)).collect();
        Ok(v?.try_into().map_err(|_| ()).unwrap())
    }
}

// Blocked TODO: https://github.com/rust-lang/rust/issues/37653
//
// Implement faster encoding of &[u8] or more generally any &[bytemuck::Pod] that encodes the same.
impl<T: Encode> Encode for [T] {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.len().encode(Gamma, writer)?;
        for t in self.iter() {
            t.encode(encoding, writer)?;
        }
        Ok(())
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.as_slice().encode(encoding, writer)
    }
}

// Blocked TODO: https://github.com/rust-lang/rust/issues/37653
//
// Implement faster decoding of Vec<u8> or more generally any Vec<bytemuck::Pod> that encodes the same.
impl<T: Decode> Decode for Vec<T> {
    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        let len = usize::decode(Gamma, reader)?;
        guard_len::<T>(len, reader)?;

        // This is faster than extend for some reason.
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode(encoding, reader)?)
        }
        Ok(vec)
    }
}

macro_rules! impl_collection {
    ($collection: ident $(,$bound: ident)*) => {
        impl<T: Encode $(+ $bound)*> Encode for std::collections::$collection<T> {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.len().encode(Gamma, writer)?;
                for t in self {
                    t.encode(encoding, writer)?;
                }
                Ok(())
            }
        }

        impl<T: Decode $(+ $bound)*> Decode for std::collections::$collection<T> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                let len = usize::decode(Gamma, reader)?;
                guard_len::<T>(len, reader)?;

                (0..len).map(|_| Ok(T::decode(encoding, reader)?)).collect()
            }
        }
    }
}

impl_collection!(VecDeque);
impl_collection!(HashSet, Hash, Eq);
impl_collection!(BTreeSet, Ord);
impl_collection!(BinaryHeap, Ord);
impl_collection!(LinkedList);

impl Encode for str {
    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.len().encode(Gamma, writer)?;
        writer.write_bytes(self.as_bytes());
        Ok(())
    }
}

impl Encode for String {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.as_str().encode(encoding, writer)
    }
}

impl Decode for String {
    #[inline(never)]
    fn decode(_: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        let len = usize::decode(Gamma, reader)?;
        let bytes = reader.read_bytes(len)?.to_vec();
        String::from_utf8(bytes).map_err(|_| E::Invalid("utf8").e())
    }
}

macro_rules! impl_map {
    ($collection: ident $(,$bound: ident)*) => {
        impl<K: Encode, V: Encode> Encode for std::collections::$collection<K, V> {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.len().encode(Gamma, writer)?;
                for (k, v) in self.iter() {
                    k.encode(encoding, writer)?;
                    v.encode(encoding, writer)?;
                }
                Ok(())
            }
        }

        impl<K: Decode $(+ $bound)*, V: Decode> Decode for std::collections::$collection<K, V> {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                let len = usize::decode(Gamma, reader)?;
                guard_len::<(K, V)>(len, reader)?;

                (0..len)
                    .map(|_| Ok((K::decode(encoding, reader)?, V::decode(encoding, reader)?)))
                    .collect()
            }
        }
    }
}

impl_map!(HashMap, Hash, Eq);
impl_map!(BTreeMap, Ord);

macro_rules! impl_ipvx_addr {
    ($addr: ident, $bytes: expr) => {
        impl Encode for $addr {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.octets().encode(encoding, writer)
            }
        }

        impl Decode for $addr {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(Self::from(<[u8; $bytes] as Decode>::decode(
                    encoding, reader,
                )?))
            }
        }
    };
}

impl_ipvx_addr!(Ipv4Addr, 4);
impl_ipvx_addr!(Ipv6Addr, 16);
impl_either!(IpAddr, V4, V6, is_ipv6);

macro_rules! impl_socket_addr_vx {
    ($addr: ident $(,$extra: expr)*) => {
        impl Encode for $addr {
            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.ip().encode(encoding, writer)?;
                self.port().encode(encoding, writer)
            }
        }

        impl Decode for $addr {
            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(Self::new(
                    Decode::decode(encoding, reader)?,
                    Decode::decode(encoding, reader)?
                    $(,$extra)*
                ))
            }
        }
    }
}

impl_socket_addr_vx!(SocketAddrV4);
impl_socket_addr_vx!(SocketAddrV6, 0, 0);
impl_either!(SocketAddr, V4, V6, is_ipv6);

impl<T> Encode for PhantomData<T> {
    fn encode(&self, _: impl Encoding, _: &mut impl Write) -> Result<()> {
        Ok(())
    }
}

impl<T> Decode for PhantomData<T> {
    fn decode(_: impl Encoding, _: &mut impl Read) -> Result<Self> {
        Ok(PhantomData)
    }
}

// TODO Cell, Duration, maybe RefCell, maybe Range/RangeInclusive/Bound.

// Allows `&str` and `&[T]` to implement encode.
impl<'a, T: Encode + ?Sized> Encode for &'a T {
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        T::encode(self, encoding, writer)
    }
}

impl Encode for () {
    fn encode(&self, _: impl Encoding, _: &mut impl Write) -> Result<()> {
        Ok(())
    }
}

impl Decode for () {
    fn decode(_: impl Encoding, _: &mut impl Read) -> Result<Self> {
        Ok(())
    }
}

macro_rules! impl_tuples {
    ($($len:expr => ($($n:tt $name:ident)+))+) => {
        $(
            impl<$($name),+> Encode for ($($name,)+)
            where
                $($name: Encode,)+
            {
                fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    $(
                        self.$n.encode(encoding, writer)?;
                    )+
                    Ok(())
                }
            }

            impl<$($name),+> Decode for ($($name,)+)
            where
                $($name: Decode,)+
            {
                #[allow(non_snake_case)]
                fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                    $(
                        let $name = Decode::decode(encoding, reader)?;
                    )+
                    Ok(($($name,)+))
                }
            }
        )+
    }
}

impl_tuples! {
    1 => (0 T0)
    2 => (0 T0 1 T1)
    3 => (0 T0 1 T1 2 T2)
    4 => (0 T0 1 T1 2 T2 3 T3)
    5 => (0 T0 1 T1 2 T2 3 T3 4 T4)
    6 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5)
    7 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6)
    8 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7)
    9 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8)
    10 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9)
    11 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10)
    12 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11)
    13 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12)
    14 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13)
    15 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14)
    16 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14 15 T15)
}
