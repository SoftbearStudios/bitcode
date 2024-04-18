use crate::coder::{Buffer, Decoder, Encoder, View};
use crate::consume::expect_eof;
use crate::Error;
use alloc::vec::Vec;
use core::num::NonZeroUsize;

mod array;
mod duration;
mod empty;
mod impls;
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
    pub use crate::coder::{uninit_field, Buffer, Decoder, Encoder, Result, View};
    pub use crate::derive::variant::{VariantDecoder, VariantEncoder};
    pub use crate::derive::{Decode, Encode};
    pub fn invalid_enum_variant<T>() -> Result<T> {
        crate::error::err("invalid enum variant")
    }
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
}
