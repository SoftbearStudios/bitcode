use crate::bool::{BoolDecoder, BoolEncoder};
use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::array::{ArrayDecoder, ArrayEncoder};
use crate::derive::empty::EmptyCoder;
use crate::derive::map::{MapDecoder, MapEncoder};
use crate::derive::option::{OptionDecoder, OptionEncoder};
use crate::derive::result::{ResultDecoder, ResultEncoder};
use crate::derive::smart_ptr::{DerefEncoder, FromDecoder};
use crate::derive::vec::{VecDecoder, VecEncoder};
use crate::derive::{Decode, Encode};
use crate::f32::{F32Decoder, F32Encoder};
use crate::int::{CheckedIntDecoder, IntDecoder, IntEncoder};
use crate::str::{StrDecoder, StrEncoder};
use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::num::*;

macro_rules! impl_both {
    ($t:ty, $encoder:ident, $decoder:ident) => {
        impl Encode for $t {
            type Encoder = $encoder;
        }
        impl<'a> Decode<'a> for $t {
            type Decoder = $decoder<'a>;
        }
    };
}
pub(crate) use impl_both;
impl_both!(bool, BoolEncoder, BoolDecoder);
impl_both!(f32, F32Encoder, F32Decoder);
impl_both!(String, StrEncoder, StrDecoder);

macro_rules! impl_int {
    ($($t:ty),+) => {
        $(
            impl Encode for $t {
                type Encoder = IntEncoder<$t>;
            }
            impl<'a> Decode<'a> for $t {
                type Decoder = IntDecoder<'a, $t>;
            }
        )+
    }
}
impl_int!(u8, u16, u32, u64, u128, usize);
impl_int!(i8, i16, i32, i64, i128, isize);
// TODO F64Encoder (once F32Encoder is sufficiently optimized).
impl Encode for f64 {
    type Encoder = IntEncoder<u64>;
}
impl<'a> Decode<'a> for f64 {
    type Decoder = IntDecoder<'a, u64>;
}

macro_rules! impl_checked_int {
    ($($a:ty => $b:ty),+) => {
        $(
            impl Encode for $a {
                type Encoder = IntEncoder<$b>;
            }
            impl<'a> Decode<'a> for $a {
                type Decoder = CheckedIntDecoder<'a, $a, $b>;
            }
        )+
    }
}
impl_checked_int!(NonZeroU8 => u8, NonZeroU16 => u16, NonZeroU32 => u32, NonZeroU64 => u64, NonZeroU128 => u128, NonZeroUsize => usize);
impl_checked_int!(NonZeroI8 => i8, NonZeroI16 => i16, NonZeroI32 => i32, NonZeroI64 => i64, NonZeroI128 => i128, NonZeroIsize => isize);
impl_checked_int!(char => u32);

macro_rules! impl_t {
    ($t:ident, $encoder:ident, $decoder:ident) => {
        impl<T: Encode> Encode for $t<T> {
            type Encoder = $encoder<T>;
        }
        impl<'a, T: Decode<'a>> Decode<'a> for $t<T> {
            type Decoder = $decoder<'a, T>;
        }
    };
}
impl_t!(LinkedList, VecEncoder, VecDecoder);
impl_t!(Option, OptionEncoder, OptionDecoder);
impl_t!(Vec, VecEncoder, VecDecoder);
impl_t!(VecDeque, VecEncoder, VecDecoder);

macro_rules! impl_smart_ptr {
    ($(::$ptr: ident)*) => {
        impl<T: Encode + ?Sized> Encode for $(::$ptr)*<T> {
            type Encoder = DerefEncoder<T>;
        }

        impl<'a, T: Decode<'a>> Decode<'a> for $(::$ptr)*<T> {
            type Decoder = FromDecoder<'a, T>;
        }

        impl<'a, T: Decode<'a>> Decode<'a> for $(::$ptr)*<[T]> {
            // TODO avoid Vec<T> allocation for Rc<[T]> and Arc<[T]>.
            type Decoder = FromDecoder<'a, Vec<T>>;
        }

        impl<'a> Decode<'a> for $(::$ptr)*<str> {
            // TODO avoid String allocation for Rc<str> and Arc<str>.
            type Decoder = FromDecoder<'a, String>;
        }
    }
}
impl_smart_ptr!(::alloc::boxed::Box);
impl_smart_ptr!(::alloc::rc::Rc);
impl_smart_ptr!(::alloc::sync::Arc);

impl<T: Encode, const N: usize> Encode for [T; N] {
    type Encoder = ArrayEncoder<T, N>;
}
impl<'a, T: Decode<'a>, const N: usize> Decode<'a> for [T; N] {
    type Decoder = ArrayDecoder<'a, T, N>;
}

// Convenience impls copied from serde etc. Makes Box<T: Encode> work on Box<[T]>.
impl<T: Encode> Encode for [T] {
    type Encoder = VecEncoder<T>;
}
impl Encode for str {
    type Encoder = StrEncoder;
}

// Partial zero copy deserialization like serde.
impl Encode for &str {
    type Encoder = StrEncoder;
}
impl<'a> Decode<'a> for &'a str {
    type Decoder = StrDecoder<'a>;
}

impl<T: Encode> Encode for BinaryHeap<T> {
    type Encoder = VecEncoder<T>;
}
impl<'a, T: Decode<'a> + Ord> Decode<'a> for BinaryHeap<T> {
    type Decoder = VecDecoder<'a, T>;
}
impl<T: Encode> Encode for BTreeSet<T> {
    type Encoder = VecEncoder<T>;
}
impl<'a, T: Decode<'a> + Ord> Decode<'a> for BTreeSet<T> {
    type Decoder = VecDecoder<'a, T>;
}

impl<K: Encode, V: Encode> Encode for BTreeMap<K, V> {
    type Encoder = MapEncoder<K, V>;
}
impl<'a, K: Decode<'a> + Ord, V: Decode<'a>> Decode<'a> for BTreeMap<K, V> {
    type Decoder = MapDecoder<'a, K, V>;
}

impl<T: Encode, E: Encode> Encode for core::result::Result<T, E> {
    type Encoder = ResultEncoder<T, E>;
}
impl<'a, T: Decode<'a>, E: Decode<'a>> Decode<'a> for core::result::Result<T, E> {
    type Decoder = ResultDecoder<'a, T, E>;
}

#[cfg(feature = "std")]
mod with_std {
    use super::*;
    use crate::derive::convert::impl_convert;
    use core::hash::{BuildHasher, Hash};
    use std::collections::{HashMap, HashSet};

    impl<T: Encode, S> Encode for HashSet<T, S> {
        type Encoder = VecEncoder<T>;
    }
    impl<'a, T: Decode<'a> + Eq + Hash, S: BuildHasher + Default> Decode<'a> for HashSet<T, S> {
        type Decoder = VecDecoder<'a, T>;
    }
    impl<K: Encode, V: Encode, S> Encode for HashMap<K, V, S> {
        type Encoder = MapEncoder<K, V>;
    }
    impl<'a, K: Decode<'a> + Eq + Hash, V: Decode<'a>, S: BuildHasher + Default> Decode<'a>
        for HashMap<K, V, S>
    {
        type Decoder = MapDecoder<'a, K, V>;
    }

    macro_rules! impl_ipvx_addr {
        ($addr: ident, $repr: ident) => {
            impl_convert!(std::net::$addr, $repr);
        };
    }

    impl_ipvx_addr!(Ipv4Addr, u32);
    impl_ipvx_addr!(Ipv6Addr, u128);
    impl_convert!(std::net::IpAddr, crate::derive::ip_addr::IpAddrConversion);
    impl_convert!(
        std::net::SocketAddrV4,
        crate::derive::ip_addr::SocketAddrV4Conversion
    );
    impl_convert!(
        std::net::SocketAddrV6,
        crate::derive::ip_addr::SocketAddrV6Conversion
    );
    impl_convert!(
        std::net::SocketAddr,
        crate::derive::ip_addr::SocketAddrConversion
    );
}

impl<T> Encode for PhantomData<T> {
    type Encoder = EmptyCoder;
}
impl<'a, T> Decode<'a> for PhantomData<T> {
    type Decoder = EmptyCoder;
}

macro_rules! impl_tuples {
    ($(($($n:tt $name:ident)*))+) => {
        $(
            #[allow(unused, clippy::unused_unit)]
            const _: () = {
                impl<$($name: Encode,)*> Encode for ($($name,)*) {
                    type Encoder = TupleEncoder<$($name,)*>;
                }

                pub struct TupleEncoder<$($name: Encode,)*>(
                    $($name::Encoder,)*
                );

                impl<$($name: Encode,)*> Default for TupleEncoder<$($name,)*> {
                    fn default() -> Self {
                        Self(
                            $($name::Encoder::default(),)*
                        )
                    }
                }

                impl<$($name: Encode,)*> Encoder<($($name,)*)> for TupleEncoder<$($name,)*> {
                    #[inline(always)]
                    fn encode(&mut self, t: &($($name,)*)) {
                        $(
                            self.$n.encode(&t.$n);
                        )*
                    }

                    // #[inline(always)]
                    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item=&'a ($($name,)*)> + Clone) where ($($name,)*): 'a {
                        $(
                            self.$n.encode_vectored(i.clone().map(|t| &t.$n));
                        )*
                    }
                }

                impl<$($name: Encode,)*> Buffer for TupleEncoder<$($name,)*> {
                    fn collect_into(&mut self, out: &mut Vec<u8>) {
                        $(
                            self.$n.collect_into(out);
                        )*
                    }

                    fn reserve(&mut self, length: NonZeroUsize) {
                        $(
                            self.$n.reserve(length);
                        )*
                    }
                }

                impl<'a, $($name: Decode<'a>,)*> Decode<'a> for ($($name,)*) {
                    type Decoder = TupleDecoder<'a, $($name,)*>;
                }

                pub struct TupleDecoder<'a, $($name: Decode<'a>,)*>(
                    $($name::Decoder,)*
                    core::marker::PhantomData<&'a ()>,
                );

                impl<'a, $($name: Decode<'a>,)*> Default for TupleDecoder<'a, $($name,)*> {
                    fn default() -> Self {
                        Self(
                            $($name::Decoder::default(),)*
                            Default::default(),
                        )
                    }
                }

                impl<'a, $($name: Decode<'a>,)*> Decoder<'a, ($($name,)*)> for TupleDecoder<'a, $($name,)*> {
                    #[inline(always)]
                    fn decode_in_place(&mut self, out: &mut MaybeUninit<($($name,)*)>) {
                        $(
                            self.$n.decode_in_place(crate::coder::uninit_field!(out.$n: $name));
                        )*
                    }
                }

                impl<'a, $($name: Decode<'a>,)*> View<'a> for TupleDecoder<'a, $($name,)*> {
                    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
                        $(
                            self.$n.populate(input, length)?;
                        )*
                        Ok(())
                    }
                }
            };
        )+
    }
}

impl_tuples! {
    ()
    (0 T0)
    (0 T0 1 T1)
    (0 T0 1 T1 2 T2)
    (0 T0 1 T1 2 T2 3 T3)
    (0 T0 1 T1 2 T2 3 T3 4 T4)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14)
    (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14 15 T15)
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec::Vec;

    type Tuple = (u64, u32, u8, i32, u8, u16, i8, (u8, u8, u8, u8), i8);
    fn bench_data() -> Vec<(Tuple, Option<String>)> {
        crate::random_data(1000)
            .into_iter()
            .map(|t: Tuple| (t, None))
            .collect()
    }
    crate::bench_encode_decode!(tuple_vec: Vec<_>);
}
