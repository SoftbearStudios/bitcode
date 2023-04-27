// Exports for derive macro. #[doc(hidden)] because not stable between versions.

pub use crate::code::*;
pub use crate::encoding::*;
pub use crate::read::Read;
pub use crate::write::Write;
pub use crate::Error;

#[cfg(any(test, feature = "serde"))]
pub use crate::serde::de::deserialize_compat;
#[cfg(any(test, feature = "serde"))]
pub use crate::serde::ser::serialize_compat;
#[cfg(any(test, feature = "serde"))]
pub use serde::{de::DeserializeOwned, Serialize};

// TODO only define once.
pub type Result<T> = std::result::Result<T, Error>;

pub fn invalid_variant() -> Error {
    crate::E::Invalid("enum variant").e()
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use crate::{Decode, Encode};

    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Recursive {
        a: Option<Box<Recursive>>,
        b: Option<Box<Self>>,
    }

    trait ParamTrait {
        type Bar: Encode + Decode;
    }

    #[derive(Encode, Decode)]
    struct Param<T: ParamTrait> {
        a: Option<T::Bar>,
    }

    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Empty;

    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Tuple(usize, u8);

    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Generic<T>(usize, T);

    #[derive(Debug, PartialEq, Encode, Decode)]
    struct FooInner {
        foo: u8,
        #[bitcode_hint(gamma)]
        bar: usize,
        baz: String,
    }

    #[derive(Debug, PartialEq, Encode, Decode)]
    #[allow(unused)]
    enum Foo {
        #[bitcode_hint(frequency = 100)]
        A,
        #[bitcode_hint(frequency = 10)]
        B(String),
        C {
            #[bitcode_hint(gamma)]
            baz: usize,
            qux: f32,
        },
        #[bitcode_hint(fixed)]
        Foo(FooInner, #[bitcode_hint(gamma)] i64),
        #[bitcode_hint(expected_range = "0..10")]
        Tuple(Tuple),
        Empty(Empty),
    }

    #[derive(Encode, Decode)]
    enum Never {}

    #[derive(Copy, Clone, Debug, PartialEq, Encode, Decode)]
    enum XYZ {
        #[bitcode_hint(frequency = 2)]
        X,
        Y,
        Z,
    }

    #[test]
    fn test_encode_x() {
        let v = [XYZ::X; 16];
        let encoded = crate::encode(&v).unwrap();
        assert_eq!(encoded.len(), 2);

        let decoded: [XYZ; 16] = crate::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_encode_y() {
        let v = [XYZ::Y; 16];
        let encoded = crate::encode(&v).unwrap();
        assert_eq!(encoded.len(), 4);

        let decoded: [XYZ; 16] = crate::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }
}
