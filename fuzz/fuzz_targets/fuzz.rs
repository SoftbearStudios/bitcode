#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate bitcode;
use bitcode::{Decode, Encode};
use bitvec::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::ffi::CString;
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let (start, data) = data.split_at(3);

    let mut bv = BitVec::<u8, Lsb0>::default();
    for byte in data {
        let boolean = match byte {
            0 => false,
            1 => true,
            _ => return,
        };
        bv.push(boolean);
    }
    let data = bv.as_raw_slice();

    macro_rules! test {
        ($typ1: expr, $typ2: expr, $data: expr, $($typ: ty),*) => {
            {
                let mut j = 0;
                $(
                    let mut buffer = bitcode::Buffer::new();

                    if j == $typ1 {
                        for _ in 0..2 {
                            if $typ2 == 0 {
                                if let Ok(de) = buffer.decode::<$typ>(data) {
                                    let data2 = buffer.encode(&de).unwrap();
                                    let de2 = bitcode::decode::<$typ>(&data2).unwrap();
                                    assert_eq!(de, de2);
                                }
                            } else if $typ2 == 1 {
                                if let Ok(de) = buffer.deserialize::<$typ>(data) {
                                    let data2 = buffer.serialize(&de).unwrap();
                                    let de2 = bitcode::deserialize::<$typ>(&data2).unwrap();
                                    assert_eq!(de, de2);
                                }
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
                                [$typ; 7],
                                [$typ; 8],
                                ([bool; 1], $typ),
                                ([bool; 2], $typ),
                                ([bool; 3], $typ),
                                ([bool; 4], $typ),
                                ([bool; 5], $typ),
                                ([bool; 6], $typ),
                                ([bool; 7], $typ),
                                Option<$typ>,
                                Vec<$typ>,
                                HashMap<u16, $typ>
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

    #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
    enum Enum {
        A,
        B,
        C(u16),
        D { a: u8, b: u8 },
        E(String),
        F,
        G(#[bitcode_hint(expected_range = "0.0..1.0")] BitsEqualF32),
        H(#[bitcode_hint(expected_range = "0.0..1.0")] BitsEqualF64),
        I(#[bitcode_hint(expected_range = "0..32")] u8),
        J(#[bitcode_hint(expected_range = "3..51")] u16),
        K(#[bitcode_hint(expected_range = "200..5000")] u32),
        L(#[bitcode_hint(gamma)] i8),
        M(#[bitcode_hint(gamma)] u64),
        N(#[bitcode_hint(ascii)] String),
        O(#[bitcode_hint(ascii_lowercase)] String),
        P(BTreeMap<u16, u8>),
        Q(Duration),
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
        CString,
        Enum
    );
});
