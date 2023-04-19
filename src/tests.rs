use crate::bit_buffer::BitBuffer;
use crate::de::{deserialize_internal, ZST_LIMIT};
use crate::ser::serialize_internal;
use crate::word_buffer::WordBuffer;
use crate::{Buffer, E};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;

#[test]
fn test_buffer_with_capacity() {
    assert_eq!(Buffer::with_capacity(0).capacity(), 0);

    let mut buf = Buffer::with_capacity(1016);
    let enough_cap = buf.capacity();
    let bytes = buf.serialize(&"a".repeat(997 + 16)).unwrap();
    assert_eq!(bytes.len(), enough_cap);
    assert_eq!(buf.capacity(), enough_cap);

    let mut buf = Buffer::with_capacity(1016);
    let small_cap = buf.capacity();
    let bytes = buf.serialize(&"a".repeat(997 + 19)).unwrap();
    assert_ne!(bytes.len(), small_cap);
    assert_ne!(buf.capacity(), small_cap);
}

fn the_same_inner<T: Clone + Debug + PartialEq + Serialize + DeserializeOwned>(
    t: T,
    buf: &mut Buffer,
) {
    let serialized = {
        let a = serialize_internal(&mut BitBuffer::default(), &t)
            .unwrap()
            .to_vec();
        let b = serialize_internal(&mut WordBuffer::default(), &t)
            .unwrap()
            .to_vec();
        assert_eq!(a, b);

        let c = buf.serialize(&t).unwrap().to_vec();
        assert_eq!(a, c);
        a
    };

    let a: T =
        deserialize_internal(&mut BitBuffer::default(), &serialized).expect("BitBuffer error");
    let b: T =
        deserialize_internal(&mut WordBuffer::default(), &serialized).expect("WordBuffer error");
    let c: T = buf
        .deserialize(&serialized)
        .expect("Buffer::deserialize error");

    assert_eq!(t, a);
    assert_eq!(t, b);
    assert_eq!(t, c);

    let mut bytes = serialized.clone();
    bytes.push(0);
    assert_eq!(
        deserialize_internal::<T>(&mut BitBuffer::default(), &bytes),
        Err(E::ExpectedEof.e())
    );
    assert_eq!(
        deserialize_internal::<T>(&mut WordBuffer::default(), &bytes),
        Err(E::ExpectedEof.e())
    );

    let mut bytes = serialized.clone();
    if bytes.pop().is_some() {
        assert_eq!(
            deserialize_internal::<T>(&mut BitBuffer::default(), &bytes),
            Err(E::Eof.e())
        );
        assert_eq!(
            deserialize_internal::<T>(&mut WordBuffer::default(), &bytes),
            Err(E::Eof.e())
        );
    }
}

fn the_same_once<T: Clone + Debug + PartialEq + Serialize + DeserializeOwned>(t: T) {
    the_same_inner(t, &mut Buffer::new());
}

fn the_same<T: Clone + Debug + PartialEq + Serialize + DeserializeOwned>(t: T) {
    let mut buf = Buffer::new();
    the_same_inner(t.clone(), &mut buf);
    #[cfg(miri)]
    const END: usize = 2;
    #[cfg(not(miri))]
    const END: usize = 65;
    for i in 0..END {
        the_same_inner(vec![t.clone(); i], &mut buf);
    }
}

#[test]
fn fuzz1() {
    assert!(crate::deserialize::<Vec<i64>>(&[64]).is_err());
}

#[test]
fn fuzz2() {
    assert!(crate::deserialize::<Vec<u8>>(&[0, 0, 0, 1]).is_err());
}

#[test]
fn fuzz3() {
    use bitvec::prelude::*;

    #[rustfmt::skip]
    let bits = bitvec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut bits2 = BitVec::<u8, Lsb0>::new();
    bits2.extend_from_bitslice(&bits);
    let bytes = bits2.as_raw_slice();

    assert!(crate::deserialize::<Vec<()>>(bytes).is_err());
}

#[test]
fn test_reddit() {
    #[derive(Serialize)]
    #[allow(dead_code)]
    enum Variant {
        Three = 3,
        Zero = 0,
        Two = 2,
        One = 1,
    }

    assert_eq!(crate::serialize(&Variant::Three).unwrap().len(), 1);
}

#[test]
fn test_negative_isize() {
    the_same_once(-5isize);
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
fn test_array_string() {
    use arrayvec::ArrayString;
    let short = ArrayString::<5>::from("abcde").unwrap();
    the_same(short);

    let long = ArrayString::<150>::from(&"abcde".repeat(30)).unwrap();
    the_same(long);
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn test_zst() {
    fn is_ok<T: Serialize + DeserializeOwned>(v: Vec<T>) -> bool {
        let ser = crate::serialize(&v).unwrap();
        crate::deserialize::<Vec<T>>(&ser).is_ok()
    }
    assert!(is_ok(vec![0u8; ZST_LIMIT]));
    assert!(is_ok(vec![0u8; ZST_LIMIT]));
    assert!(!is_ok(vec![(); ZST_LIMIT + 1]));
    assert!(is_ok(vec![0u8; ZST_LIMIT + 1]));
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn test_chars() {
    for n in 0..=char::MAX as u32 {
        if let Some(c) = char::from_u32(n) {
            the_same_once(c);
            the_same_once([c; 2]);
        }
    }
}

// Everything below this comment was derived from bincode:
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
