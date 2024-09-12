#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate bitcode;
use arrayvec::{ArrayString, ArrayVec};
use bitcode::{Decode, DecodeOwned, Encode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::num::NonZeroU32;
use std::time::Duration;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddr, SocketAddrV6};

#[inline(never)]
fn test_derive<T: Debug + PartialEq + Encode + DecodeOwned>(data: &[u8]) {
    let mut buffer = bitcode::Buffer::default();
    let [split, data @ ..] = data else {
        return;
    };

    // Call buffer.decode twice with the same data or different data.
    // Same data makes sure it's pure by checking that both runs return Ok or Err.
    // Different data catches invalid states left behind.
    let eq_slices = *split == 0;
    let slices = if eq_slices {
        [data, data]
    } else {
        let (a, b) = data.split_at((*split as usize).min(data.len()));
        [a, b]
    };

    let mut previous = None;
    for data in slices {
        let data = data.to_vec(); // Detect dangling pointers to data in buffer.
        let current = if let Ok(de) = buffer.decode::<T>(&data) {
            let data2 = buffer.encode::<T>(&de);
            let de2 = bitcode::decode::<T>(&data2).unwrap();
            assert_eq!(de, de2);
            true
        } else {
            false
        };
        if eq_slices {
            if let Some(previous) = std::mem::replace(&mut previous, Some(current)) {
                assert_eq!(previous, current);
            }
        }
    }
}

#[inline(never)]
fn test_serde<T: Debug + PartialEq + Serialize + DeserializeOwned>(data: &[u8]) {
    if let Ok(de) = bitcode::deserialize::<T>(data) {
        let data2 = bitcode::serialize(&de).unwrap();
        let de2 = bitcode::deserialize::<T>(&data2).unwrap();
        assert_eq!(de, de2);
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let (start, data) = data.split_at(3);

    macro_rules! test {
        ($typ1: expr, $typ2: expr, $data: expr, $($typ: ty,)*) => {
            {
                let mut j = 0;
                $(
                    if j == $typ1 {
                        if $typ2 == 0 {
                            test_derive::<$typ>(data);
                        } else if $typ2 == 1 {
                            test_serde::<$typ>(data);
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
        ($typ0: expr, $typ1: expr, $typ2: expr, $data: expr, $($typ: ty,)*) => {
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
                                Option<$typ>,
                                Vec<$typ>,
                                HashMap<u16, $typ>,
                                ArrayVec<$typ, 0>,
                                ArrayVec<$typ, 5>,
                                Result<$typ, u32>,
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
        ArrayVec<u8, 70>,
        Duration,
        Ipv4Addr,
        Ipv6Addr,
        IpAddr,
        SocketAddrV4,
        SocketAddrV6,
        SocketAddr,
    );
});
