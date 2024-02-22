use crate::coder::{Buffer, Decoder, Encoder, View};
use crate::consume::expect_eof;
use crate::Error;
use std::num::NonZeroUsize;

mod array;
mod empty;
mod impls;
mod map;
mod option;
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

/// Encodes a `T:` [`Encode`] into a [`Vec<u8>`].
///
/// **Warning:** The format is subject to change between major versions.
pub fn encode<T: Encode + ?Sized>(t: &T) -> Vec<u8> {
    let mut encoder = T::Encoder::default();
    encoder.reserve(NonZeroUsize::new(1).unwrap());

    #[inline(never)]
    fn encode_inner<T: Encode + ?Sized>(encoder: &mut T::Encoder, t: &T) {
        encoder.encode(t);
    }
    encode_inner(&mut encoder, t);
    encoder.collect()
}

/// Decodes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Decode`].
///
/// **Warning:** The format is subject to change between major versions.
pub fn decode<'a, T: Decode<'a>>(mut bytes: &'a [u8]) -> Result<T, Error> {
    let mut decoder = T::Decoder::default();
    decoder.populate(&mut bytes, 1)?;
    expect_eof(bytes)?;
    #[inline(never)]
    fn decode_inner<'a, T: Decode<'a>>(decoder: &mut T::Decoder) -> T {
        decoder.decode()
    }
    Ok(decode_inner(&mut decoder))
}

/// A buffer for reusing allocations between multiple calls to [`EncodeBuffer::encode`].
pub struct EncodeBuffer<T: Encode + ?Sized> {
    encoder: T::Encoder,
    out: Vec<u8>,
}

// #[derive(Default)] bounds T: Default.
impl<T: Encode + ?Sized> Default for EncodeBuffer<T> {
    fn default() -> Self {
        Self {
            encoder: Default::default(),
            out: Default::default(),
        }
    }
}

impl<T: Encode + ?Sized> EncodeBuffer<T> {
    /// Encodes a `T:` [`Encode`] into a [`&[u8]`][`prim@slice`].
    ///
    /// Can reuse allocations when called multiple times on the same [`EncodeBuffer`].
    ///
    /// **Warning:** The format is subject to change between major versions.
    pub fn encode<'a>(&'a mut self, t: &T) -> &'a [u8] {
        // TODO dedup with encode.
        self.encoder.reserve(NonZeroUsize::new(1).unwrap());
        #[inline(never)]
        fn encode_inner<T: Encode + ?Sized>(encoder: &mut T::Encoder, t: &T) {
            encoder.encode(t);
        }
        encode_inner(&mut self.encoder, t);
        self.out.clear();
        self.encoder.collect_into(&mut self.out);
        self.out.as_slice()
    }
}

/// A buffer for reusing allocations between multiple calls to [`DecodeBuffer::decode`].
///
/// TODO don't bound [`DecodeBuffer`] to decode's `&'a [u8]`.
pub struct DecodeBuffer<'a, T: Decode<'a>>(<T as Decode<'a>>::Decoder);

impl<'a, T: Decode<'a>> Default for DecodeBuffer<'a, T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<'a, T: Decode<'a>> DecodeBuffer<'a, T> {
    /// Decodes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Decode`].
    ///
    /// Can reuse allocations when called multiple times on the same [`DecodeBuffer`].
    ///
    /// **Warning:** The format is subject to change between major versions.
    pub fn decode(&mut self, mut bytes: &'a [u8]) -> Result<T, Error> {
        // TODO dedup with decode.
        self.0.populate(&mut bytes, 1)?;
        expect_eof(bytes)?;
        #[inline(never)]
        fn decode_inner<'a, T: Decode<'a>>(decoder: &mut T::Decoder) -> T {
            decoder.decode()
        }
        let ret = decode_inner(&mut self.0);
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Decode, Encode};

    #[test]
    fn decode() {
        macro_rules! test {
            ($v:expr, $t:ty) => {
                let encoded = super::encode::<$t>(&$v);
                println!("{:<24} {encoded:?}", stringify!($t));
                assert_eq!($v, super::decode::<$t>(&encoded).unwrap());
            };
        }

        test!(("abc", "123"), (&str, &str));
        test!(Vec::<Option<i16>>::new(), Vec<Option<i16>>);
        test!(vec![None, Some(1), None], Vec<Option<i16>>);
    }

    #[derive(Encode, Decode)]
    enum Never {}

    #[derive(Encode, Decode)]
    enum One {
        A(u8),
    }

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
