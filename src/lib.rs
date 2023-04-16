#![cfg_attr(test, feature(test))]
#![forbid(unsafe_code)]

//! Bitcode is a crate for encoding and decoding using a tinier
//! binary serialization strategy. You can easily go from having
//! an object in memory, quickly serialize it to bytes, and then
//! deserialize it back just as fast!
//!
//! The format is not necessarily stable between versions. If you want
//! a stable format, consider [bincode](https://docs.rs/bincode/latest/bincode/).
//!
//! ### Usage
//!
//! ```edition2021
//! // The object that we will serialize.
//! let target: Vec<String> = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
//!
//! let encoded: Vec<u8> = bitcode::serialize(&target).unwrap();
//! let decoded: Vec<String> = bitcode::deserialize(&encoded).unwrap();
//! assert_eq!(target, decoded);
//! ```

// Actually required see https://doc.rust-lang.org/beta/unstable-book/library-features/test.html
#[cfg(test)]
extern crate core;
#[cfg(test)]
extern crate test;

use de::{deserialize_with, read::ReadWithImpl};
use ser::{serialize_with, write::WriteWithImpl};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

#[cfg(all(test, not(miri)))]
mod benches;
mod de;
mod nightly;
mod ser;
#[cfg(test)]
mod tests;

/// Serializes a `T:` [`Serialize`] into a [`Vec<u8>`].
///
/// **Warning:** The format is subject to change between versions.
pub fn serialize<T: ?Sized>(t: &T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    serialize_with::<WriteWithImpl>(t)
}

/// Deserializes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Deserialize`].
///
/// **Warning:** The format is subject to change between versions.
pub fn deserialize<'a, T>(bytes: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    deserialize_with::<'a, T, ReadWithImpl>(bytes)
}

/// (De)serialization errors.
///
/// # Debug mode
///
/// In debug mode, the error contains a reason.
///
/// # Release mode
///
/// In release mode, the error is a zero-sized type for efficiency.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Error(ErrorImpl);

#[cfg(not(debug_assertions))]
type ErrorImpl = ();

#[cfg(debug_assertions)]
type ErrorImpl = E;

impl Error {
    /// Replaces an invalid message. E.g. read_variant_index calls read_len but converts
    /// `E::Invalid("length")` to `E::Invalid("variant index")`.
    pub(crate) fn map_invalid(self, _s: &'static str) -> Self {
        #[cfg(debug_assertions)]
        return Self(match self.0 {
            E::Invalid(_) => E::Invalid(_s),
            _ => self.0,
        });
        #[cfg(not(debug_assertions))]
        self
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Inner error that can be converted to [`Error`] with [`E::e`].
#[derive(Debug, PartialEq)]
pub(crate) enum E {
    #[cfg(debug_assertions)]
    Custom(String),
    Eof,
    ExpectedEof,
    Invalid(&'static str),
    NotSupported(&'static str),
}

impl E {
    fn e(self) -> Error {
        #[cfg(debug_assertions)]
        return Error(self);
        #[cfg(not(debug_assertions))]
        Error(())
    }
}

type Result<T> = std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        return Display::fmt(&self.0, f);
        #[cfg(not(debug_assertions))]
        f.write_str("bitcode error")
    }
}

impl Display for E {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(debug_assertions)]
            Self::Custom(s) => write!(f, "custom: {s}"),
            Self::Eof => write!(f, "eof"),
            Self::ExpectedEof => write!(f, "expected eof"),
            Self::Invalid(s) => write!(f, "invalid {s}"),
            Self::NotSupported(s) => write!(f, "{s} is not supported"),
        }
    }
}

impl std::error::Error for Error {}

impl serde::ser::Error for Error {
    fn custom<T>(_msg: T) -> Self
    where
        T: Display,
    {
        #[cfg(debug_assertions)]
        return Self(E::Custom(_msg.to_string()));
        #[cfg(not(debug_assertions))]
        Self(())
    }
}

impl serde::de::Error for Error {
    fn custom<T>(_msg: T) -> Self
    where
        T: Display,
    {
        #[cfg(debug_assertions)]
        return Self(E::Custom(_msg.to_string()));
        #[cfg(not(debug_assertions))]
        Self(())
    }
}
