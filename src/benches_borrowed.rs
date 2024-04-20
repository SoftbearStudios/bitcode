use crate::benches::{bench_data, Data, DataEnum, MAX_DATA_ENUMS};
use alloc::vec::Vec;
use core::array;
use serde::{Deserialize, Serialize};
use test::{black_box, Bencher};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "derive", derive(crate::Encode, crate::Decode))]
struct Data2<'a> {
    entity: &'a str,
    x: u8,
    y: bool,
    item: &'a str,
    z: u16,
    e: [DataEnum2<'a>; MAX_DATA_ENUMS],
}

impl<'a> From<&'a Data> for Data2<'a> {
    fn from(v: &'a Data) -> Self {
        Self {
            entity: &v.entity,
            x: v.x,
            y: v.y,
            item: &v.item,
            z: v.z,
            e: array::from_fn(|i| v.e.get(i).map(From::from).unwrap_or(DataEnum2::Bar)),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "derive", derive(crate::Encode, crate::Decode))]
enum DataEnum2<'a> {
    Bar,
    Baz(&'a str),
    Foo(Option<u8>),
}

impl<'a> From<&'a DataEnum> for DataEnum2<'a> {
    fn from(v: &'a DataEnum) -> Self {
        match v {
            DataEnum::Bar => Self::Bar,
            DataEnum::Baz(v) => Self::Baz(v),
            DataEnum::Foo(v) => Self::Foo(*v),
        }
    }
}

fn bench_data2(bench_data: &[Data]) -> Vec<Data2> {
    bench_data.iter().map(From::from).collect()
}

#[bench]
fn bench_bincode_serialize(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);
    let mut buffer = vec![];

    b.iter(|| {
        let buffer = black_box(&mut buffer);
        buffer.clear();
        bincode::serialize_into(buffer, black_box(&data)).unwrap();
    })
}

#[bench]
fn bench_bincode_deserialize(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);
    let mut bytes = vec![];
    bincode::serialize_into(&mut bytes, &data).unwrap();

    assert_eq!(
        bincode::deserialize::<Vec<Data2>>(&mut bytes.as_slice()).unwrap(),
        data
    );
    b.iter(|| {
        black_box(bincode::deserialize::<Vec<Data2>>(&mut black_box(bytes.as_slice())).unwrap());
    })
}

#[cfg(feature = "derive")]
#[bench]
fn bench_bitcode_encode(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);
    let mut buffer = crate::Buffer::default();

    b.iter(|| {
        black_box(buffer.encode(black_box(&data)));
    })
}

#[cfg(feature = "derive")]
#[bench]
fn bench_bitcode_decode(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);
    let mut encode_buffer = crate::Buffer::default();
    let bytes = encode_buffer.encode(&data);

    let mut decode_buffer = crate::Buffer::default();
    assert_eq!(decode_buffer.decode::<Vec<Data2>>(bytes).unwrap(), data);
    b.iter(|| {
        black_box(
            decode_buffer
                .decode::<Vec<Data2>>(black_box(bytes))
                .unwrap(),
        );
    })
}

#[cfg(feature = "serde")]
#[bench]
fn bench_bitcode_serialize(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);

    b.iter(|| {
        black_box(crate::serialize(black_box(&data)).unwrap());
    })
}

#[cfg(feature = "serde")]
#[bench]
fn bench_bitcode_deserialize(b: &mut Bencher) {
    let data = bench_data();
    let data = bench_data2(&data);
    let bytes = crate::serialize(&data).unwrap();

    assert_eq!(crate::deserialize::<Vec<Data2>>(&bytes).unwrap(), data);
    b.iter(|| {
        black_box(crate::deserialize::<Vec<Data2>>(black_box(bytes.as_slice())).unwrap());
    })
}
