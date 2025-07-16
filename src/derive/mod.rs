use crate::coder::{Buffer, Decoder, Encoder, View};
use crate::consume::expect_eof;
use crate::Error;
use alloc::vec::Vec;
use core::num::NonZeroUsize;

mod array;
mod atomic;
pub(crate) mod convert;
mod duration;
mod empty;
mod impls;
// TODO: When ip_in_core has been stable (https://github.com/rust-lang/rust/issues/108443)
// for long enough, remove feature check.
#[cfg(feature = "std")]
mod ip_addr;
mod map;
mod option;
mod result;
mod smart_ptr;
mod variant;
pub(crate) mod vec;

// For derive macro.
#[cfg(feature = "derive")]
#[doc(hidden)]
pub mod __private {
    extern crate alloc;
    pub use crate::coder::{uninit_field, Buffer, Decoder, Encoder, Result, View};
    pub use crate::derive::variant::{VariantDecoder, VariantEncoder};
    pub use crate::derive::{Decode, Encode};
    pub fn invalid_enum_variant<T>() -> Result<T> {
        crate::error::err("invalid enum variant")
    }
    pub use alloc::vec::Vec;
}

/// A type which can be encoded to bytes with [`encode`].
///
/// Use `#[derive(Encode)]` to implement.
pub trait Encode {
    #[doc(hidden)]
    type Encoder: Encoder<Self>;
}

/// A type which can be decoded from bytes with [`decode`].
///
/// Use `#[derive(Decode)]` to implement.
pub trait Decode<'a>: Sized {
    #[doc(hidden)]
    type Decoder: Decoder<'a, Self>;
}

/// A type which can be decoded without borrowing any bytes from the input.
///
/// This type is a shorter version of `for<'de> Decode<'de>`.
pub trait DecodeOwned: for<'de> Decode<'de> {}
impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}

// Stop #[inline(always)] of Encoder::encode/Decoder::decode since 90% of the time is spent in these
// functions, and we don't want extra code interfering with optimizations.
#[inline(never)]
fn encode_inline_never<T: Encode + ?Sized>(encoder: &mut T::Encoder, t: &T) {
    encoder.encode(t);
}
#[inline(never)]
fn decode_inline_never<'a, T: Decode<'a>>(decoder: &mut T::Decoder) -> T {
    decoder.decode()
}

/// Encodes a `T:` [`Encode`] into a [`Vec<u8>`].
///
/// **Warning:** The format is subject to change between major versions.
pub fn encode<T: Encode + ?Sized>(t: &T) -> Vec<u8> {
    let mut encoder = T::Encoder::default();
    encoder.reserve(NonZeroUsize::new(1).unwrap());
    encode_inline_never(&mut encoder, t);
    encoder.collect()
}

/// Decodes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Decode`].
///
/// **Warning:** The format is subject to change between major versions.
pub fn decode<'a, T: Decode<'a>>(mut bytes: &'a [u8]) -> Result<T, Error> {
    let mut decoder = T::Decoder::default();
    decoder.populate(&mut bytes, 1)?;
    expect_eof(bytes)?;
    Ok(decode_inline_never(&mut decoder))
}

impl crate::buffer::Buffer {
    /// Like [`encode`], but saves allocations between calls.
    pub fn encode<'a, T: Encode + ?Sized>(&'a mut self, t: &T) -> &'a [u8] {
        // Safety: Encoders don't have any lifetimes (they don't contain T either).
        let encoder = unsafe { self.registry.get_non_static::<T::Encoder>() };
        encoder.reserve(NonZeroUsize::new(1).unwrap());
        encode_inline_never(encoder, t);
        self.out.clear();
        encoder.collect_into(&mut self.out);
        self.out.as_slice()
    }

    /// Like [`decode`], but saves allocations between calls.
    pub fn decode<'a, T: Decode<'a>>(&mut self, mut bytes: &'a [u8]) -> Result<T, Error> {
        // Safety: Decoders have dangling pointers to `bytes` from previous calls which haven't been
        // cleared. This isn't an issue in practice because they remain as pointers in FastSlice and
        // aren't dereferenced. If we wanted to be safer we could clear all the decoders but this
        // would result in lots of extra code to maintain and a performance/binary size hit.
        // To detect misuse we run miri tests/cargo fuzz where bytes goes out of scope between calls.
        let decoder = unsafe { self.registry.get_non_static::<T::Decoder>() };
        decoder.populate(&mut bytes, 1)?;
        expect_eof(bytes)?;
        Ok(decode_inline_never(decoder))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Decode, Encode};
    use alloc::vec::Vec;

    #[test]
    fn decode() {
        macro_rules! test {
            ($v:expr, $t:ty) => {
                let v = $v;
                let encoded = super::encode::<$t>(&v);
                #[cfg(feature = "std")]
                println!("{:<24} {encoded:?}", stringify!($t));
                assert_eq!(v, super::decode::<$t>(&encoded).unwrap());
            };
        }

        test!(("abc", "123"), (&str, &str));
        test!(Vec::<Option<i16>>::new(), Vec<Option<i16>>);
        test!(vec![None, Some(1), None], Vec<Option<i16>>);
        test!((0, 1), (usize, isize));
        test!(vec![true; 255], Vec<bool>);
        test!([0, 1], [u8; 2]);
        test!([0, 1, 2], [u8; 3]);
        test!([0, -1, 0, -1, 0, -1, 0], [i8; 7]);
        test!([], [u8; 0]);
    }

    #[derive(Encode, Decode)]
    enum Never {}

    #[derive(Encode, Decode)]
    enum One {
        A(u8),
    }

    // cargo expand --lib --tests | grep -A15 Two
    #[derive(Encode, Decode)]
    enum Two {
        A(u8),
        B(i8),
    }

    #[derive(Encode, Decode)]
    struct TupleStruct(u8, i8);

    #[derive(Encode, Decode)]
    struct Generic<T>(T);

    #[derive(Encode, Decode)]
    struct GenericManual<T>(#[bitcode(bound_type = "T")] T);

    #[derive(Encode, Decode)]
    struct GenericWhere<A, B>(A, B)
    where
        A: From<B>;

    #[derive(Encode, Decode)]
    struct Lifetime<'a>(&'a str);

    #[derive(Encode, Decode, Debug, PartialEq)]
    struct LifetimeSkipped<'a>(#[bitcode(skip)] &'a str);

    #[derive(Encode, Decode)]
    struct LifetimeWhere<'a, 'b>(&'a str, &'b str)
    where
        'a: 'b;

    #[derive(Encode, Decode)]
    struct ConstGeneric<const N: usize>([u8; N]);

    #[derive(Encode, Decode)]
    struct Empty;

    #[derive(Encode, Decode)]
    struct AssociatedConst([u8; Self::N]);
    impl AssociatedConst {
        const N: usize = 1;
    }

    #[derive(Encode, Decode)]
    struct AssociatedConstTrait([u8; <Self as Trait>::N]);
    trait Trait {
        const N: usize;
    }
    impl Trait for AssociatedConstTrait {
        const N: usize = 1;
    }

    #[test]
    fn skipped_fields() {
        macro_rules! test_skip {
            ($a:expr, $b:expr, $t:ty) => {
                let v = $a;
                let encoded = super::encode::<$t>(&v);
                #[cfg(feature = "std")]
                println!("{:<24} {encoded:?}", stringify!($t));
                assert_eq!($b, super::decode::<$t>(&encoded).unwrap());
            };
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct LifetimeSkipped<'a>(#[bitcode(skip)] &'a str);

        let skipped_string = alloc::string::String::from("I'm skipped!");
        let lifetime = LifetimeSkipped(&skipped_string);
        test_skip!(lifetime, LifetimeSkipped(""), LifetimeSkipped);

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipStruct {
            pub a: u32,
            #[bitcode(skip)]
            pub b: u32,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipTuple(bool, #[bitcode(skip)] u32, u8, #[bitcode(skip)] u8, i32);

        #[derive(Encode, Decode, Debug, PartialEq)]
        enum SkipEnumTuple {
            A(u8, u32),
            B(bool, #[bitcode(skip)] u32, u8, #[bitcode(skip)] u8, i32),
        }

        #[derive(Default, Debug, PartialEq)]
        struct Skipped(u32);

        #[derive(Encode, Decode, Debug, PartialEq)]
        enum SkipEnumStruct {
            A {
                a: u8,
                #[bitcode(skip)]
                b: Skipped,
                c: u8,
                #[bitcode(skip)]
                d: u8,
                e: u8,
            },
            B,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipGeneric<A, B> {
            present: A,
            #[bitcode(skip)]
            skipped: B,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct PartialSkipGeneric<A, B> {
            present: A,
            also_present: B,
            #[bitcode(skip)]
            skipped: B,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipAll {
            #[bitcode(skip)]
            skipped: u8,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipAllGeneric<A> {
            #[bitcode(skip)]
            skipped: A,
        }

        #[derive(Default, Debug, PartialEq)]
        struct Indirect<A> {
            field: A,
        }

        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipIndirectGeneric<A> {
            #[bitcode(skip)]
            skipped: Indirect<A>,
        }

        test_skip!(
            SkipStruct { a: 231, b: 9696 },
            SkipStruct { a: 231, b: 0 },
            SkipStruct
        );
        test_skip!(
            SkipTuple(true, 23, 231, 42, -13),
            SkipTuple(true, 0, 231, 0, -13),
            SkipTuple
        );
        test_skip!(
            SkipEnumTuple::B(true, 23, 231, 42, -42),
            SkipEnumTuple::B(true, 0, 231, 0, -42),
            SkipEnumTuple
        );
        test_skip!(
            SkipEnumStruct::A {
                a: 1,
                b: Skipped(2),
                c: 3,
                d: 4,
                e: 5
            },
            SkipEnumStruct::A {
                a: 1,
                b: Skipped(0),
                c: 3,
                d: 0,
                e: 5
            },
            SkipEnumStruct
        );
        test_skip! {
            SkipAll {
                skipped: 42u8,
            },
            SkipAll {
                skipped: 0u8,
            },
            SkipAll
        }
        assert_eq!(bitcode::encode(&SkipAll { skipped: 42u8 }).len(), 0);
        test_skip!(
            SkipGeneric {
                present: 42u8,
                skipped: Skipped(231),
            },
            SkipGeneric {
                present: 42u8,
                skipped: Skipped(0),
            },
            SkipGeneric<u8, Skipped>
        );
        test_skip!(
            PartialSkipGeneric {
                present: 42u8,
                also_present: 231i32,
                skipped: 77i32,
            },
            PartialSkipGeneric {
                present: 42u8,
                also_present: 231i32,
                skipped: 0i32,
            },
            PartialSkipGeneric<u8, i32>
        );
        test_skip! {
            SkipAllGeneric {
                skipped: 42i32,
            },
            SkipAllGeneric {
                skipped: 0i32,
            },
            SkipAllGeneric<i32>
        }
        test_skip! {
            SkipIndirectGeneric {
                skipped: Indirect{ field: 42i32 },
            },
            SkipIndirectGeneric {
                skipped: Indirect{ field: 0i32 },
            },
            SkipIndirectGeneric<i32>
        }
        assert_eq!(bitcode::encode(&SkipAllGeneric { skipped: 42u8 }).len(), 0);
    }

    #[test]
    fn skipped_fields_regression() {
        #[derive(Encode, Decode, Default, Debug, PartialEq)]
        pub struct Indirect<A>(A);
        #[derive(Encode, Decode, Debug, PartialEq)]
        struct SkipGeneric<A> {
            #[bitcode(bound_type = "Indirect<A>")]
            present: Indirect<A>,
        }
    }
}
