use crate::code::{optimized_dec, optimized_enc, Decode, Encode};
use crate::encoding::{Encoding, Fixed, Gamma};
use crate::guard::guard_len;
use crate::nightly::{max, min, utf8_char_width};
use crate::read::Read;
use crate::write::Write;
use crate::{Result, E};
use std::hash::Hash;
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::num::*;

macro_rules! impl_enc_const {
    ($v:expr) => {
        const ENCODE_MIN: usize = $v;
        const ENCODE_MAX: usize = $v;
    };
}

macro_rules! impl_enc_size {
    ($t:ty) => {
        impl_enc_const!(std::mem::size_of::<$t>() * u8::BITS as usize);
    };
}

macro_rules! impl_enc_same {
    ($other:ty) => {
        const ENCODE_MIN: usize = <$other>::ENCODE_MIN;
        const ENCODE_MAX: usize = <$other>::ENCODE_MAX;
    };
}

macro_rules! impl_dec_from_enc {
    () => {
        const DECODE_MIN: usize = Self::ENCODE_MIN;
        const DECODE_MAX: usize = Self::ENCODE_MAX;
    };
}

macro_rules! impl_dec_same {
    ($other:ty) => {
        const DECODE_MIN: usize = <$other>::DECODE_MIN;
        const DECODE_MAX: usize = <$other>::DECODE_MAX;
    };
}

impl Encode for bool {
    impl_enc_const!(1);

    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        writer.write_bit(*self);
        Ok(())
    }
}

impl Decode for bool {
    impl_dec_from_enc!();

    fn decode(_: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        reader.read_bit()
    }
}

macro_rules! impl_uints {
    ($($int: ty),*) => {
        $(
            impl Encode for $int {
                impl_enc_size!($int);

                fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    encoding.write_word(writer, (*self).into(), <$int>::BITS as usize);
                    Ok(())
                }
            }

            impl Decode for $int {
                impl_dec_from_enc!();

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
                impl_enc_size!($int);

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
                impl_dec_from_enc!();

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
            impl_enc_size!($b);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                (*self as $b).encode(encoding, writer)
            }
        }

        impl Decode for $a {
            impl_dec_from_enc!();

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
            impl_enc_size!($a);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                encoding.$write(writer, *self);
                Ok(())
            }
        }

        impl Decode for $a {
            impl_dec_from_enc!();

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
                impl_enc_size!($a);

                fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    (self.get() - 1).encode(Fixed, writer)
                }
            }

            impl Decode for $a {
                impl_dec_from_enc!();

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
    const ENCODE_MIN: usize = 8;
    const ENCODE_MAX: usize = 32;

    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        let mut buf = [0; 4];
        let n = self.encode_utf8(&mut buf).len();
        writer.write_bits(u32::from_le_bytes(buf) as u64, n * u8::BITS as usize);
        Ok(())
    }
}

impl Decode for char {
    impl_dec_from_enc!();

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
    const ENCODE_MIN: usize = 1;
    const ENCODE_MAX: usize = T::ENCODE_MAX.saturating_add(1);

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        if let Some(t) = self {
            fn encode_some<T: Encode>(
                t: &T,
                encoding: impl Encoding,
                writer: &mut impl Write,
            ) -> Result<()> {
                optimized_enc!(encoding, writer);
                enc!(true, bool);
                enc!(t, T);
                end_enc!();
                Ok(())
            }
            encode_some(t, encoding, writer)
        } else {
            false.encode(encoding, writer)?;
            Ok(())
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    const DECODE_MIN: usize = 1;
    const DECODE_MAX: usize = T::DECODE_MAX.saturating_add(1);

    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        optimized_dec!(encoding, reader);
        dec!(v, bool);
        if v {
            dec!(t, T);
            end_dec!();
            Ok(Some(t))
        } else {
            end_dec!();
            Ok(None)
        }
    }
}

macro_rules! impl_either {
    ($typ: path, $a: ident, $a_t:ty, $b:ident, $b_t: ty, $is_b: ident $(,$($generic: ident);*)*) => {
        impl $(<$($generic: Encode),*>)* Encode for $typ {
            const ENCODE_MIN: usize = 1 + min(<$a_t>::ENCODE_MIN, <$b_t>::ENCODE_MIN);
            const ENCODE_MAX: usize = max(<$a_t>::ENCODE_MAX, <$b_t>::ENCODE_MAX).saturating_add(1);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                match self {
                    Self::$a(a) => {
                        debug_assert!(!self.$is_b());
                        optimized_enc!(encoding, writer);
                        enc!(false, bool);
                        enc!(a, $a_t);
                        end_enc!();
                        Ok(())
                    },
                    Self::$b(b) => {
                        debug_assert!(self.$is_b());
                        optimized_enc!(encoding, writer);
                        enc!(true, bool);
                        enc!(b, $b_t);
                        end_enc!();
                        Ok(())
                    },
                }
            }
        }

        impl $(<$($generic: Decode),*>)* Decode for $typ {
            const DECODE_MIN: usize = 1 + min(<$a_t>::DECODE_MIN, <$b_t>::DECODE_MIN);
            const DECODE_MAX: usize = max(<$a_t>::DECODE_MAX, <$b_t>::DECODE_MAX).saturating_add(1);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                optimized_dec!(encoding, reader);
                dec!(v, bool);
                Ok(if v {
                    dec!(b, $b_t);
                    end_dec!();
                    Self::$b(b)
                } else {
                    dec!(a, $a_t);
                    end_dec!();
                    Self::$a(a)
                })
            }
        }
    }
}

impl_either!(std::result::Result<T, E>, Ok, T, Err, E, is_err, T ; E);

macro_rules! impl_wrapper {
    ($(::$ptr: ident)*) => {
        impl<T: Encode> Encode for $(::$ptr)*<T> {
            impl_enc_same!(T);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                T::encode(&self.0, encoding, writer)
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<T> {
            impl_dec_same!(T);

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
            impl_enc_same!(T);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                T::encode(self, encoding, writer)
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<T> {
            impl_dec_same!(T);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(T::decode(encoding, reader)?.into())
            }
        }

        impl<T: Decode> Decode for $(::$ptr)*<[T]> {
            impl_dec_same!(Vec<T>);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(Vec::<T>::decode(encoding, reader)?.into())
            }
        }

        impl Decode for $(::$ptr)*<str> {
            impl_dec_same!(String);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                Ok(String::decode(encoding, reader)?.into())
            }
        }
    }
}

impl_smart_ptr!(::std::boxed::Box);
impl_smart_ptr!(::std::rc::Rc);
impl_smart_ptr!(::std::sync::Arc);

// Writes multiple elements per flush. TODO use on VecDeque::as_slices.
#[inline(always)] // If only #[inline] optimized_enc_tests::bench_array is slow.
fn encode_elements<T: Encode>(
    elements: &[T],
    encoding: impl Encoding,
    writer: &mut impl Write,
) -> Result<()> {
    if T::ENCODE_MAX == 0 {
        return Ok(()); // Nothing to serialize.
    }
    let chunk_size = 64 / T::ENCODE_MAX;

    if chunk_size > 1 && encoding.is_fixed() {
        let mut buf = crate::register_buffer::RegisterWriter::new(writer);

        let chunks = elements.chunks_exact(chunk_size);
        let remainder = chunks.remainder();

        for chunk in chunks {
            for t in chunk {
                t.encode(encoding, &mut buf.inner)?;
            }
            buf.flush();
        }

        if !remainder.is_empty() {
            for t in remainder {
                t.encode(encoding, &mut buf.inner)?;
            }
            buf.flush();
        }
    } else {
        for t in elements.iter() {
            t.encode(encoding, writer)?
        }
    }
    Ok(())
}

// Reads multiple elements per flush.
#[inline]
fn decode_elements<T: Decode>(
    len: usize,
    encoding: impl Encoding,
    reader: &mut impl Read,
) -> Result<Vec<T>> {
    let chunk_size = if encoding.is_fixed() && T::DECODE_MAX != 0 {
        64 / T::DECODE_MAX
    } else {
        1
    };

    if chunk_size >= 2 {
        let chunks = len / chunk_size;
        let remainder = len % chunk_size;

        let mut ret = Vec::with_capacity(len);
        let mut buf = crate::register_buffer::RegisterReader::new(reader);

        for _ in 0..chunks {
            buf.refill()?;
            let r = &mut buf.inner;

            // This avoids checking if allocation is needed for every item for chunks divisible by 8.
            // Adding more impls for other sizes slows down this case for some reason.
            if chunk_size % 8 == 0 {
                for _ in 0..chunk_size / 8 {
                    ret.extend([
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                        T::decode(encoding, r)?,
                    ])
                }
            } else {
                for _ in 0..chunk_size {
                    ret.push(T::decode(encoding, r)?)
                }
            }
        }

        buf.refill()?;
        for _ in 0..remainder {
            ret.push(T::decode(encoding, &mut buf.inner)?);
        }
        buf.advance_reader();

        Ok(ret)
    } else {
        // This is faster than extend for some reason.
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode(encoding, reader)?)
        }
        Ok(vec)
    }
}

impl<const N: usize, T: Encode> Encode for [T; N] {
    const ENCODE_MIN: usize = T::ENCODE_MIN * N;
    const ENCODE_MAX: usize = T::ENCODE_MAX.saturating_mul(N);

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        encode_elements(self, encoding, writer)
    }
}

impl<const N: usize, T: Decode> Decode for [T; N] {
    const DECODE_MIN: usize = T::DECODE_MIN * N;
    const DECODE_MAX: usize = T::DECODE_MAX.saturating_mul(N);

    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        // TODO find a safe way to decode an array without allocating.
        // Maybe use ArrayVec, but that would require another dep.
        Ok(decode_elements(N, encoding, reader)?
            .try_into()
            .ok()
            .unwrap())
    }
}

// Blocked TODO: https://github.com/rust-lang/rust/issues/37653
//
// Implement faster encoding of &[u8] or more generally any &[bytemuck::Pod] that encodes the same.
impl<T: Encode> Encode for [T] {
    const ENCODE_MIN: usize = 1;
    const ENCODE_MAX: usize = usize::MAX;

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.len().encode(Gamma, writer)?;
        encode_elements(self, encoding, writer)
    }
}

impl<T: Encode> Encode for Vec<T> {
    impl_enc_same!([T]);

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.as_slice().encode(encoding, writer)
    }
}

// Blocked TODO: https://github.com/rust-lang/rust/issues/37653
//
// Implement faster decoding of Vec<u8> or more generally any Vec<bytemuck::Pod> that encodes the same.
impl<T: Decode> Decode for Vec<T> {
    const DECODE_MIN: usize = 1;
    const DECODE_MAX: usize = usize::MAX;

    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
        let len = usize::decode(Gamma, reader)?;
        guard_len::<T>(len, reader)?;
        decode_elements(len, encoding, reader)
    }
}

macro_rules! impl_collection {
    ($collection: ident $(,$bound: ident)*) => {
        impl<T: Encode $(+ $bound)*> Encode for std::collections::$collection<T> {
            impl_enc_same!([T]);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.len().encode(Gamma, writer)?;
                for t in self {
                    t.encode(encoding, writer)?;
                }
                Ok(())
            }
        }

        impl<T: Decode $(+ $bound)*> Decode for std::collections::$collection<T> {
            impl_dec_same!(Vec<T>);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                let len = usize::decode(Gamma, reader)?;
                guard_len::<T>(len, reader)?;

                (0..len).map(|_| T::decode(encoding, reader)).collect()
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
    const ENCODE_MIN: usize = 1;
    const ENCODE_MAX: usize = usize::MAX;

    fn encode(&self, _: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.len().encode(Gamma, writer)?;
        writer.write_bytes(self.as_bytes());
        Ok(())
    }
}

impl Encode for String {
    impl_enc_same!(str);

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        self.as_str().encode(encoding, writer)
    }
}

impl Decode for String {
    impl_dec_from_enc!();

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
            impl_enc_same!([(K, V)]);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.len().encode(Gamma, writer)?;
                for t in self.iter() {
                    t.encode(encoding, writer)?;
                }
                Ok(())
            }
        }

        impl<K: Decode $(+ $bound)*, V: Decode> Decode for std::collections::$collection<K, V> {
            impl_dec_same!(Vec<(K, V)>);

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                let len = usize::decode(Gamma, reader)?;
                guard_len::<(K, V)>(len, reader)?;

                (0..len)
                    .map(|_| <(K, V)>::decode(encoding, reader))
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
            impl_enc_const!($bytes * u8::BITS as usize);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                self.octets().encode(encoding, writer)
            }
        }

        impl Decode for $addr {
            impl_dec_from_enc!();

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
impl_either!(IpAddr, V4, Ipv4Addr, V6, Ipv6Addr, is_ipv6);

macro_rules! impl_socket_addr_vx {
    ($addr:ident, $ip_addr:ident, $bytes:expr $(,$extra: expr)*) => {
        impl Encode for $addr {
            impl_enc_const!(($bytes) * u8::BITS as usize);

            fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                optimized_enc!(encoding, writer);
                enc!(self.ip(), $ip_addr);
                enc!(self.port(), u16);
                end_enc!();
                Ok(())
            }
        }

        impl Decode for $addr {
            impl_dec_from_enc!();

            fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                optimized_dec!(encoding, reader);
                dec!(ip, $ip_addr);
                dec!(port, u16);
                end_dec!();
                Ok(Self::new(
                    ip,
                    port
                    $(,$extra)*
                ))
            }
        }
    }
}

impl_socket_addr_vx!(SocketAddrV4, Ipv4Addr, 4 + 2);
impl_socket_addr_vx!(SocketAddrV6, Ipv6Addr, 16 + 2, 0, 0);
impl_either!(SocketAddr, V4, SocketAddrV4, V6, SocketAddrV6, is_ipv6);

impl<T> Encode for PhantomData<T> {
    impl_enc_const!(0);

    fn encode(&self, _: impl Encoding, _: &mut impl Write) -> Result<()> {
        Ok(())
    }
}

impl<T> Decode for PhantomData<T> {
    impl_dec_from_enc!();

    fn decode(_: impl Encoding, _: &mut impl Read) -> Result<Self> {
        Ok(PhantomData)
    }
}

// TODO Cell, Duration, maybe RefCell, maybe Range/RangeInclusive/Bound.

// Allows `&str` and `&[T]` to implement encode.
impl<'a, T: Encode + ?Sized> Encode for &'a T {
    impl_enc_same!(T);

    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
        T::encode(self, encoding, writer)
    }
}

impl Encode for () {
    impl_enc_const!(0);

    fn encode(&self, _: impl Encoding, _: &mut impl Write) -> Result<()> {
        Ok(())
    }
}

impl Decode for () {
    impl_dec_from_enc!();

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
                const ENCODE_MIN: usize = $(<$name>::ENCODE_MIN +)+ 0;
                const ENCODE_MAX: usize = 0usize $(.saturating_add(<$name>::ENCODE_MAX))+;

                fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()> {
                    optimized_enc!(encoding, writer);
                    $(
                        enc!(self.$n, $name);
                    )+
                    end_enc!();
                    Ok(())
                }
            }

            impl<$($name),+> Decode for ($($name,)+)
            where
                $($name: Decode,)+
            {
                const DECODE_MIN: usize = $(<$name>::DECODE_MIN +)+ 0;
                const DECODE_MAX: usize = 0usize $(.saturating_add(<$name>::DECODE_MAX))+;

                #[allow(non_snake_case)]
                fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self> {
                    optimized_dec!(encoding, reader);
                    $(
                        dec!($name, $name);
                    )+
                    end_dec!();
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
