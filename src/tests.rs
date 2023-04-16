use crate::de::deserialize_with;
use crate::de::read::{BitSliceImpl, DeVec};
use crate::ser::serialize_with;
use crate::ser::write::{BitVecImpl, SerVec};
use crate::{deserialize, E};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::*;

fn the_same_inner<T: Clone + Debug + PartialEq + Serialize + DeserializeOwned>(t: T) {
    let serialized = {
        let a = serialize_with::<SerVec>(&t).unwrap();
        let b = serialize_with::<BitVecImpl>(&t).unwrap();
        assert_eq!(a, b);
        a
    };

    let a: T = deserialize_with::<T, DeVec>(&serialized).expect("DeVec error");
    let b: T = deserialize_with::<T, BitSliceImpl>(&serialized).expect("BitSliceImpl error");

    assert_eq!(t, a);
    assert_eq!(t, b);
    assert_eq!(a, b);

    let mut bytes = serialized.clone();
    bytes.push(0);
    assert_eq!(
        deserialize_with::<T, DeVec>(&bytes),
        Err(E::ExpectedEof.e())
    );
    assert_eq!(
        deserialize_with::<T, BitSliceImpl>(&bytes),
        Err(E::ExpectedEof.e())
    );

    let mut bytes = serialized.clone();
    if bytes.pop().is_some() {
        assert_eq!(deserialize_with::<T, DeVec>(&bytes), Err(E::Eof.e()));

        assert_eq!(deserialize_with::<T, BitSliceImpl>(&bytes), Err(E::Eof.e()));
    }
}

fn the_same<T: Clone + Debug + PartialEq + Serialize + DeserializeOwned>(t: T) {
    the_same_inner(t.clone());
    for i in 0..65 {
        the_same_inner(vec![t.clone(); i]);
    }
}

#[test]
fn fuzz_1() {
    assert!(deserialize::<Vec<i64>>(&[64]).is_err());
}

#[test]
fn fuzz_2() {
    assert!(deserialize::<Vec<u8>>(&[0, 0, 0, 1]).is_err());
}

#[test]
fn test_negative_isize() {
    the_same_inner(-5isize);
}

#[test]
fn test_zst_vec() {
    for i in (0..100).step_by(9) {
        the_same(vec![(); i]);
    }
}

#[test]
fn test_long_string() {
    the_same("abcde".repeat(25))
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn test_chars() {
    for n in 0..=char::MAX as u32 {
        if let Some(c) = char::from_u32(n) {
            the_same_inner(c);
            the_same_inner([c; 2]);
        }
    }
}

// Everything below this comment
// was derived from bincode:
// https://github.com/bincode-org/bincode/blob/v1.x/tests/test.rs

#[test]
fn test_numbers() {
    // unsigned positive
    the_same(5u8);
    the_same(5u16);
    the_same(5u32);
    the_same(5u64);
    the_same(u64::MAX - 5);
    the_same(u64::MAX);
    the_same(5usize);
    // signed positive
    the_same(5i8);
    the_same(5i16);
    the_same(5i32);
    the_same(5i64);
    the_same(5isize);
    // signed negative
    the_same(-5i8);
    the_same(-5i16);
    the_same(-5i32);
    the_same(-5i64);
    the_same(i64::MAX);
    the_same(i64::MAX - 5);
    the_same(i64::MIN);
    the_same(i64::MIN + 5);
    the_same(-5isize);
    // floating
    the_same(-100f32);
    the_same(0f32);
    the_same(5f32);
    the_same(-100f64);
    the_same(5f64);
}

#[test]
fn test_string() {
    the_same("".to_string());
    the_same("a".to_string());
}

#[test]
fn test_tuple() {
    the_same((1isize,));
    the_same((1isize, 2isize, 3isize));
    the_same((1isize, "foo".to_string(), ()));
}

#[test]
fn test_basic_struct() {
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct Easy {
        x: isize,
        s: String,
        y: usize,
    }
    the_same(Easy {
        x: -4,
        s: "foo".to_string(),
        y: 10,
    });
}

#[test]
fn test_nested_struct() {
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct Easy {
        x: isize,
        s: String,
        y: usize,
    }
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct Nest {
        f: Easy,
        b: usize,
        s: Easy,
    }

    the_same(Nest {
        f: Easy {
            x: -1,
            s: "foo".to_string(),
            y: 20,
        },
        b: 100,
        s: Easy {
            x: -100,
            s: "bar".to_string(),
            y: 20,
        },
    });
}

#[test]
fn test_struct_newtype() {
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct NewtypeStr(usize);

    the_same(NewtypeStr(5));
}

#[test]
fn test_struct_tuple() {
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct TubStr(usize, String, f32);

    the_same(TubStr(5, "hello".to_string(), 3.2));
}

#[test]
fn test_option() {
    the_same(Some(5usize));
    the_same(Some("foo bar".to_string()));
    the_same(None::<usize>);
}

#[test]
fn test_enum() {
    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    enum TestEnum {
        NoArg,
        OneArg(usize),
        Args(usize, usize),
        AnotherNoArg,
        StructLike { x: usize, y: f32 },
    }
    the_same(TestEnum::NoArg);
    the_same(TestEnum::OneArg(4));
    //the_same(TestEnum::Args(4, 5));
    the_same(TestEnum::AnotherNoArg);
    the_same(TestEnum::StructLike { x: 4, y: 3.14159 });
    the_same(vec![
        TestEnum::NoArg,
        TestEnum::OneArg(5),
        TestEnum::AnotherNoArg,
        TestEnum::StructLike { x: 4, y: 1.4 },
    ]);
}

#[test]
fn test_vec() {
    let v: Vec<u8> = vec![];
    the_same(v);
    the_same(vec![1u64]);
    the_same(vec![1u64, 2, 3, 4, 5, 6]);
}

#[test]
fn test_map() {
    let mut m = HashMap::new();
    m.insert(4u64, "foo".to_string());
    m.insert(0u64, "bar".to_string());
    the_same(m);
}

#[test]
fn test_bool() {
    the_same(true);
    the_same(false);
}

#[test]
fn test_unicode() {
    the_same("å".to_string());
    the_same("aåååååååa".to_string());
}

#[test]
fn test_fixed_size_array() {
    the_same([24u32; 32]);
    the_same([1u64, 2, 3, 4, 5, 6, 7, 8]);
    the_same([0u8; 19]);
}
