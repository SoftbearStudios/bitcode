use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use test::black_box;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "arrayvec")]
use arrayvec::{ArrayString, ArrayVec};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "derive", derive(crate::Encode, crate::Decode))]
pub struct Data {
    #[cfg(feature = "arrayvec")]
    pub entity: ArrayString<8>,
    #[cfg(not(feature = "arrayvec"))]
    pub entity: String,

    pub x: u8,
    pub y: bool,

    #[cfg(feature = "arrayvec")]
    pub item: ArrayString<12>,
    #[cfg(not(feature = "arrayvec"))]
    pub item: String,

    pub z: u16,

    #[cfg(feature = "arrayvec")]
    pub e: ArrayVec<DataEnum, 5>,
    #[cfg(not(feature = "arrayvec"))]
    pub e: Vec<DataEnum>,
}

pub const MAX_DATA_ENUMS: usize = 5;
impl Distribution<Data> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Data {
        Data {
            entity: (*[
                "cow", "sheep", "zombie", "skeleton", "spider", "creeper", "parrot", "bee",
            ]
            .choose(rng)
            .unwrap())
            .try_into()
            .unwrap(),
            x: rng.gen(),
            y: rng.gen_bool(0.1),
            item: (*[
                "dirt",
                "stone",
                "pickaxe",
                "sand",
                "gravel",
                "shovel",
                "chestplate",
                "steak",
            ]
            .choose(rng)
            .unwrap())
            .try_into()
            .unwrap(),
            z: rng.gen(),
            e: (0..rng.gen_range(0..MAX_DATA_ENUMS))
                .map(|_| rng.gen())
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "derive", derive(crate::Encode, crate::Decode))]
pub enum DataEnum {
    Bar,
    #[cfg(feature = "arrayvec")]
    Baz(ArrayString<16>),
    #[cfg(not(feature = "arrayvec"))]
    Baz(String),
    Foo(Option<u8>),
}

impl Distribution<DataEnum> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> DataEnum {
        if rng.gen_bool(0.9) {
            DataEnum::Bar
        } else if rng.gen_bool(0.5) {
            let n = rng.gen_range(0..15);
            DataEnum::Baz(
                rng.sample_iter(rand::distributions::Alphanumeric)
                    .take(n)
                    .map(char::from)
                    .collect::<String>()
                    .as_str()
                    .try_into()
                    .unwrap(),
            )
        } else {
            DataEnum::Foo(rng.gen_bool(0.2).then(|| rng.gen()))
        }
    }
}

fn random_data(n: usize) -> Vec<Data> {
    let mut rng = ChaCha20Rng::from_seed(Default::default());
    (0..n).map(|_| rng.gen()).collect()
}

// Use bincode fixint for benchmarks because it's faster than varint.
fn bincode_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    bincode::serialize(v).unwrap()
}
fn bincode_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::deserialize(v).unwrap()
}

#[cfg(feature = "derive")]
fn bitcode_encode(v: &(impl crate::Encode + ?Sized)) -> Vec<u8> {
    crate::encode(v)
}
#[cfg(feature = "derive")]
fn bitcode_decode<T: crate::DecodeOwned>(v: &[u8]) -> T {
    crate::decode(v).unwrap()
}

#[cfg(feature = "serde")]
fn bitcode_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    crate::serialize(v).unwrap()
}
#[cfg(feature = "serde")]
fn bitcode_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    crate::deserialize(v).unwrap()
}

pub fn bench_data() -> Vec<Data> {
    random_data(crate::limit_bench_miri(1000))
}

#[cfg(any(feature = "derive", feature = "serde"))]
macro_rules! bench {
    ($serialize:ident, $deserialize:ident, $($name:ident),*) => {
        paste::paste! {
            $(
                #[bench]
                fn [<bench_ $name _$serialize>] (b: &mut test::Bencher) {
                    let data = bench_data();
                    b.iter(|| {
                        black_box([<$name _ $serialize>](black_box(&data)));
                    })
                }

                #[bench]
                fn [<bench_ $name _$deserialize>] (b: &mut test::Bencher) {
                    let data = bench_data();
                    let serialized_data = &[<$name _ $serialize>](&data);
                    assert_eq!([<$name _ $deserialize>]::<Vec<Data>>(serialized_data), data);
                    b.iter(|| {
                        black_box([<$name _ $deserialize>]::<Vec<Data>>(black_box(serialized_data)));
                    })
                }
            )*
        }
    }
}

bench!(serialize, deserialize, bincode);
#[cfg(feature = "serde")]
bench!(serialize, deserialize, bitcode);
#[cfg(feature = "derive")]
bench!(encode, decode, bitcode);

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;
    use std::time::{Duration, Instant};

    /// # With many allocations in deserialize
    /// cargo test --release --features=serde -- --show-output comparison1
    ///
    /// # With String -> ArrayString and Vec -> ArrayVec
    /// cargo test --release --all-features -- --show-output comparison1
    #[test]
    #[cfg_attr(debug_assertions, ignore = "don't run unless --include-ignored")]
    fn comparison1() {
        let data = &random_data(10000);
        let print_single = |name: &str,
                            compression: &str,
                            ser: &dyn Fn(&[Data]) -> Vec<u8>,
                            de: &dyn Fn(&[u8]) -> Vec<Data>| {
            let b = ser(&data);
            // if compression.is_empty() {
            //     print!("{name} {compression} ");
            //     println!("{}", String::from_utf8_lossy(&b).replace(char::is_control, "�"));
            //     // println!("{:?}", b);
            // }

            fn benchmark_ns(f: impl Fn()) -> usize {
                const WARMUP: usize = 2;
                let start = Instant::now();
                for _ in 0..WARMUP {
                    f();
                }
                let warmup_duration = start.elapsed();
                let per_second = (WARMUP as f32 / warmup_duration.as_secs_f32()) as usize;
                let samples: usize = (per_second / 32).max(1);
                let mut duration = Duration::ZERO;
                for _ in 0..samples {
                    let start = Instant::now();
                    f();
                    duration += start.elapsed();
                }
                duration.as_nanos() as usize / samples
            }

            let ser_time = benchmark_ns(|| {
                black_box(ser(black_box(&data)));
            }) / data.len();

            let de_time = benchmark_ns(|| {
                black_box(de(black_box(&b)));
            }) / data.len();

            println!(
                    "| {name:<16} | {compression:<12} | {:<12.1} | {ser_time:<10}     | {de_time:<10}       |",
                    b.len() as f32 / data.len() as f32,
                );
        };

        let print_results =
            |name: &str, ser: fn(&[Data]) -> Vec<u8>, de: fn(&[u8]) -> Vec<Data>| {
                for (compression, encode, decode) in compression::ALGORITHMS {
                    print_single(name, compression, &|v| encode(&ser(v)), &|v| de(&decode(v)));
                }
            };

        println!("| Format           | Compression  | Size (bytes) | Serialize (ns) | Deserialize (ns) |");
        println!("|------------------|--------------|--------------|----------------|------------------|");
        print_results("bincode", bincode_serialize, bincode_deserialize);
        print_results(
            "bincode-varint",
            |v| bincode::DefaultOptions::new().serialize(v).unwrap(),
            |v| bincode::DefaultOptions::new().deserialize(v).unwrap(),
        );
        #[cfg(feature = "serde")]
        print_results("bitcode", bitcode_serialize, bitcode_deserialize);
        #[cfg(feature = "derive")]
        print_results("bitcode-derive", bitcode_encode, bitcode_decode);
    }
}

mod compression {
    use flate2::read::DeflateDecoder;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use lz4_flex::{compress_prepend_size, decompress_size_prepended};
    use std::io::{Read, Write};

    pub static ALGORITHMS: &[(&str, fn(&[u8]) -> Vec<u8>, fn(&[u8]) -> Vec<u8>)] = &[
        ("", ToOwned::to_owned, ToOwned::to_owned),
        ("lz4", lz4_encode, lz4_decode),
        ("deflate-fast", deflate_fast_encode, deflate_decode),
        ("deflate-best", deflate_best_encode, deflate_decode),
        #[cfg(not(miri))] // zstd doesn't compile with miri big-endian.
        ("zstd-0", zstd_encode::<0>, zstd_decode),
        #[cfg(not(miri))]
        ("zstd-22", zstd_encode::<22>, zstd_decode),
    ];

    fn lz4_encode(v: &[u8]) -> Vec<u8> {
        compress_prepend_size(v)
    }

    fn lz4_decode(v: &[u8]) -> Vec<u8> {
        decompress_size_prepended(v).unwrap()
    }

    fn deflate_fast_encode(v: &[u8]) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
        e.write_all(v).unwrap();
        e.finish().unwrap()
    }

    fn deflate_best_encode(v: &[u8]) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
        e.write_all(v).unwrap();
        e.finish().unwrap()
    }

    fn deflate_decode(v: &[u8]) -> Vec<u8> {
        let mut bytes = vec![];
        DeflateDecoder::new(v).read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[cfg(not(miri))]
    fn zstd_encode<const LEVEL: i32>(v: &[u8]) -> Vec<u8> {
        zstd::stream::encode_all(v, LEVEL).unwrap()
    }

    #[cfg(not(miri))]
    fn zstd_decode(v: &[u8]) -> Vec<u8> {
        zstd::stream::decode_all(v).unwrap()
    }
}
