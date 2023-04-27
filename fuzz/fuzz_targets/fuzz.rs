#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate bitcode;
use bitcode::{Encode, Decode};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use bitvec::prelude::*;

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
                    {
                        if j == $typ1 {
                            if $typ2 == 0 {
                                if let Ok(de) = bitcode::decode::<$typ>(data) {
                                    let _ = bitcode::encode(&de).unwrap();
                                }
                            } else if $typ2 == 1 {
                                if let Ok(de) = bitcode::deserialize::<$typ>(data) {
                                    let _ = bitcode::serialize(&de).unwrap();
                                }
                            }
                        }
                        #[allow(unused)]
                        {
                            j += 1;
                        }
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

    #[derive(Serialize, Deserialize, Encode, Decode, PartialEq)]
    enum Enum {
        A,
        B,
        C(u16),
        D{a: u8, b: u8},
        E(String),
        F,
        G(#[bitcode_hint(expected_range = "0.0..1.0")] f32),
        H(#[bitcode_hint(expected_range = "0.0..1.0")] f64),
        I(#[bitcode_hint(expected_range = "0..32")] u8),
        J(#[bitcode_hint(expected_range = "3..51")] u16),
        K(#[bitcode_hint(expected_range = "200..5000")] u32),
        // E(Box<Self>)
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
        usize,
        isize,
        f32,
        f64,
        Vec<u8>,
        String,
        Enum
    );
});