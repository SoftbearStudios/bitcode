#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate bitcode;
use arrayvec::{ArrayString, ArrayVec};
use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU32;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let (start, data) = data.split_at(3);
    let mut buffer = bitcode::Buffer::default();

    macro_rules! test {
        ($typ1: expr, $typ2: expr, $data: expr, $($typ: ty),*) => {
            {
                let mut j = 0;
                $(
                    if j == $typ1 {
                        if $typ2 == 0 {
                            let mut previous = None;
                            for _ in 0..2 {
                                let data = data.to_vec(); // Detect dangling pointers to data in buffer.
                                let current = if let Ok(de) = buffer.decode::<$typ>(&data) {
                                    let data2 = buffer.encode::<$typ>(&de);
                                    let de2 = bitcode::decode::<$typ>(&data2).unwrap();
                                    assert_eq!(de, de2);
                                    true
                                } else {
                                    false
                                };
                                if let Some(previous) = std::mem::replace(&mut previous, Some(current)) {
                                    assert_eq!(previous, current);
                                }
                            }
                        } else if $typ2 == 1 {
                            if let Ok(de) = bitcode::deserialize::<$typ>(data) {
                                let data2 = bitcode::serialize(&de).unwrap();
                                let de2 = bitcode::deserialize::<$typ>(&data2).unwrap();
                                assert_eq!(de, de2);
                            }
                        }
                    }
                    #[allow(unused)]
                    {
                        j += 1;
                    }
                )*
            }
        }
    }

    macro_rules! tests {
        ($typ0: expr, $typ1: expr, $typ2: expr, $data: expr, $($typ: ty),*) => {
            {
                let mut i = 0;
                $(
                    {
                        if i == $typ0 {
                            test!(
                                $typ1,
                                $typ2,
                                $data,
                                $typ,
                                ($typ, $typ),
                                [$typ; 1],
                                [$typ; 2],
                                [$typ; 3],
                                Option<$typ>,
                                Vec<$typ>,
                                HashMap<u16, $typ>,
                                ArrayVec<$typ, 0>,
                                ArrayVec<$typ, 5>,
                                Result<$typ, u32>
                            );
                        }
                        #[allow(unused)]
                        {
                            i += 1;
                        }
                    }
                )*
            }
        }
    }

    #[rustfmt::skip]
    mod enums {
        use super::*;
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum2 { A, B }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum3 { A, B, C }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum4 { A, B, C, D }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum5 { A, B, C, D, E }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum6 { A, B, C, D, E, F }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum7 { A, B, C, D, E, F, G }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum15 { A, B, C, D, E, F, G, H, I, J, K, L, M, N, O }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum16 { A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P }
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        pub enum Enum17 { A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q }
    }
    use enums::*;

    #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
    enum Enum {
        A,
        B,
        C(u16),
        D { a: u8, b: u8 },
        E(String),
        F,
        P(BTreeMap<u16, u8>),
    }

    #[derive(Serialize, Deserialize, Encode, Decode, Debug)]
    struct BitsEqualF32(f32);

    impl PartialEq for BitsEqualF32 {
        fn eq(&self, other: &Self) -> bool {
            self.0.to_bits() == other.0.to_bits()
        }
    }

    #[derive(Serialize, Deserialize, Encode, Decode, Debug)]
    struct BitsEqualF64(f64);

    impl PartialEq for BitsEqualF64 {
        fn eq(&self, other: &Self) -> bool {
            self.0.to_bits() == other.0.to_bits()
        }
    }

    tests!(
        start[0],
        start[1],
        start[2],
        data,
        (),
        bool,
        char,
        NonZeroU32,
        u8,
        i8,
        u16,
        i16,
        u32,
        i32,
        u64,
        i64,
        u128,
        i128,
        usize,
        isize,
        BitsEqualF32,
        BitsEqualF64,
        Vec<u8>,
        String,
        Enum2,
        Enum3,
        Enum4,
        Enum5,
        Enum6,
        Enum7,
        Enum15,
        Enum16,
        Enum17,
        Enum,
        ArrayString<5>,
        ArrayString<70>,
        ArrayVec<u8, 5>,
        ArrayVec<u8, 70>
    );
});
