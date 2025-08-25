#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate bitcode;
use arrayvec::{ArrayString, ArrayVec};
use bitcode::{Decode, DecodeOwned, Encode};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::num::NonZeroU32;
use std::time::Duration;

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
                                Struct<String, $typ>,
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
        #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
        enum Enum300 {
            V1, V2, V3, V4, V5, V6, V7, V8, V9, V10,
            V11, V12, V13, V14, V15, V16, V17, V18, V19, V20,
            V21, V22, V23, V24, V25, V26, V27, V28, V29, V30,
            V31, V32, V33, V34, V35, V36, V37, V38, V39, V40,
            V41, V42, V43, V44, V45, V46, V47, V48, V49, V50,
            V51, V52, V53, V54, V55, V56, V57, V58, V59, V60,
            V61, V62, V63, V64, V65, V66, V67, V68, V69, V70,
            V71, V72, V73, V74, V75, V76, V77, V78, V79, V80,
            V81, V82, V83, V84, V85, V86, V87, V88, V89, V90,
            V91, V92, V93, V94, V95, V96, V97, V98, V99, V100,
            V101, V102, V103, V104, V105, V106, V107, V108, V109, V110,
            V111, V112, V113, V114, V115, V116, V117, V118, V119, V120,
            V121, V122, V123, V124, V125, V126, V127, V128, V129, V130,
            V131, V132, V133, V134, V135, V136, V137, V138, V139, V140,
            V141, V142, V143, V144, V145, V146, V147, V148, V149, V150,
            V151, V152, V153, V154, V155, V156, V157, V158, V159, V160,
            V161, V162, V163, V164, V165, V166, V167, V168, V169, V170,
            V171, V172, V173, V174, V175, V176, V177, V178, V179, V180,
            V181, V182, V183, V184, V185, V186, V187, V188, V189, V190,
            V191, V192, V193, V194, V195, V196, V197, V198, V199, V200,
            V201, V202, V203, V204, V205, V206, V207, V208, V209, V210,
            V211, V212, V213, V214, V215, V216, V217, V218, V219, V220,
            V221, V222, V223, V224, V225, V226, V227, V228, V229, V230,
            V231, V232, V233, V234, V235, V236, V237, V238, V239, V240,
            V241, V242, V243, V244, V245, V246, V247, V248, V249, V250,
            V251, V252, V253, V254, V255, V256, V257, V258, V259, V260,
            V261, V262, V263, V264, V265, V266, V267, V268, V269, V270,
            V271, V272, V273, V274, V275, V276, V277, V278, V279, V280,
            V281, V282, V283, V284, V285, V286, V287, V288, V289, V290,
            V291, V292, V293, V294, V295, V296, V297, V298, V299, V300,
        }
    }
    use enums::*;

    #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
    enum Enum {
        A,
        B,
        C(u16),
        D {
            a: u8,
            b: u8,
            #[serde(skip)]
            #[bitcode(skip)]
            c: u8,
        },
        E(String),
        F,
        G(
            #[bitcode(skip)]
            #[serde(skip)]
            i16,
        ),
        P(BTreeMap<u16, u8>),
    }

    #[derive(Serialize, Deserialize, Encode, Decode, Debug, PartialEq)]
    struct Struct<D, T> {
        foo: BitsEqualF32,
        #[bitcode(skip)]
        #[serde(skip)]
        bar: f32,
        baz: T,
        #[bitcode(skip)]
        #[serde(skip)]
        def: D,
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
        Enum300,
        Enum,
        ArrayString<5>,
        ArrayString<70>,
        ArrayVec<u8, 5>,
        ArrayVec<u8, 70>,
        Decimal,
        Duration,
        Ipv4Addr,
        Ipv6Addr,
        IpAddr,
        SocketAddrV4,
        SocketAddrV6,
        SocketAddr,
        time::Time,
    );
});
