use crate::{BoundedGammaEncoding, FullGammaEncoding};
use bincode::Options;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use paste::paste;
use rand::distributions::Alphanumeric;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use test::{black_box, Bencher};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Data {
    x: Option<f32>,
    y: Option<i8>,
    z: u16,
    s: String,
    e: DataEnum,
}

fn gen_len(r: &mut (impl Rng + ?Sized)) -> usize {
    (r.gen::<f32>().powi(4) * 16.0) as usize
}

impl Distribution<Data> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Data {
        let n = gen_len(rng);
        Data {
            x: rng.gen_bool(0.15).then(|| rng.gen()),
            y: rng.gen_bool(0.3).then(|| rng.gen()),
            z: rng.gen(),
            s: rng
                .sample_iter(Alphanumeric)
                .take(n)
                .map(char::from)
                .collect(),
            e: rng.gen(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
enum DataEnum {
    #[default]
    Bar,
    Baz(String),
    Foo(Option<u8>),
}

impl Distribution<DataEnum> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> DataEnum {
        if rng.gen_bool(0.9) {
            DataEnum::Bar
        } else if rng.gen_bool(0.5) {
            let n = gen_len(rng);
            DataEnum::Baz(
                rng.sample_iter(Alphanumeric)
                    .take(n)
                    .map(char::from)
                    .collect(),
            )
        } else {
            DataEnum::Foo(rng.gen_bool(0.5).then(|| rng.gen()))
        }
    }
}

fn random_data(n: usize) -> Vec<Data> {
    let mut rng = ChaCha20Rng::from_seed(Default::default());
    (0..n).map(|_| rng.gen()).collect()
}

fn bitcode_default_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    crate::serialize(v).unwrap()
}

fn bitcode_default_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    crate::deserialize(v).unwrap()
}

fn bitcode_full_gamma_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    crate::serialize_with_encoding(v, FullGammaEncoding).unwrap()
}

fn bitcode_full_gamma_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    crate::deserialize_with_encoding(FullGammaEncoding, v).unwrap()
}

fn bitcode_bounded_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    crate::serialize_with_encoding(v, BoundedGammaEncoding).unwrap()
}

fn bitcode_bounded_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    crate::deserialize_with_encoding(BoundedGammaEncoding, v).unwrap()
}

fn bincode_fixint_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    bincode::serialize(v).unwrap()
}

fn bincode_fixint_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::deserialize(v).unwrap()
}

fn bincode_varint_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    bincode::DefaultOptions::new().serialize(v).unwrap()
}

fn bincode_varint_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new().deserialize(v).unwrap()
}

fn bincode_lz4_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    compress_prepend_size(&bincode::DefaultOptions::new().serialize(v).unwrap())
}

fn bincode_lz4_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new()
        .deserialize(&decompress_size_prepended(v).unwrap())
        .unwrap()
}

fn bincode_flate2_fast_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
    bincode::DefaultOptions::new()
        .serialize_into(&mut e, v)
        .unwrap();
    e.finish().unwrap()
}

fn bincode_flate2_fast_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new()
        .deserialize_from(DeflateDecoder::new(v))
        .unwrap()
}

fn bincode_flate2_best_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
    bincode::DefaultOptions::new()
        .serialize_into(&mut e, v)
        .unwrap();
    e.finish().unwrap()
}

fn bincode_flate2_best_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode_flate2_fast_deserialize(v)
}

fn postcard_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    postcard::to_allocvec(v).unwrap()
}

fn postcard_deserialize<T: DeserializeOwned>(buf: &[u8]) -> T {
    postcard::from_bytes(buf).unwrap()
}

fn bench_data() -> Vec<Data> {
    random_data(1000)
}

fn bench_serialize(b: &mut Bencher, ser: fn(&[Data]) -> Vec<u8>) {
    let data = bench_data();
    b.iter(|| {
        black_box(ser(black_box(&data)));
    })
}

fn bench_deserialize(b: &mut Bencher, ser: fn(&[Data]) -> Vec<u8>, de: fn(&[u8]) -> Vec<Data>) {
    let data = bench_data();
    let ref serialized_data = ser(&data);
    assert_eq!(de(serialized_data), data);
    b.iter(|| {
        black_box(de(black_box(serialized_data)));
    })
}

macro_rules! bench {
    ($($name:ident),*) => {
        paste! {
            $(
                #[bench]
                fn [<bench_ $name _serialize>] (b: &mut Bencher) {
                    bench_serialize(b, [<$name _serialize>])
                }

                #[bench]
                fn [<bench_ $name _deserialize>] (b: &mut Bencher) {
                    bench_deserialize(b, [<$name _serialize>], [<$name _deserialize>])
                }
            )*
        }
    }
}

bench!(
    bitcode_default,
    bitcode_full_gamma,
    bitcode_bounded,
    bincode_fixint,
    bincode_varint,
    bincode_lz4,
    bincode_flate2_fast,
    bincode_flate2_best,
    postcard
);

#[test]
fn comparison1() {
    let ref data = random_data(10000);

    let print_results = |name: &'static str, b: Vec<u8>| {
        let zeros = 100.0 * b.iter().filter(|&&b| b == 0).count() as f32 / b.len() as f32;
        let precision = 2 - (zeros.log10().ceil() as usize).min(1);

        println!(
            "| {name:<22} | {:<12.1} | {zeros:>4.1$}%      |",
            b.len() as f32 / data.len() as f32,
            precision,
        );
    };

    println!("| Format                 | Size (bytes) | Zero Bytes |");
    println!("|------------------------|--------------|------------|");
    print_results("Bitcode", bitcode_default_serialize(data));
    print_results("Bitcode Full", bitcode_full_gamma_serialize(data));
    print_results("Bitcode Bounded", bitcode_bounded_serialize(data));
    print_results("Bincode", bincode_fixint_serialize(data));
    print_results("Bincode (Varint)", bincode_varint_serialize(data));

    // These use varint since it makes the result smaller and actually speeds up compression.
    print_results("Bincode (LZ4)", bincode_lz4_serialize(data));
    print_results(
        "Bincode (Deflate Fast)",
        bincode_flate2_fast_serialize(data),
    );
    print_results(
        "Bincode (Deflate Best)",
        bincode_flate2_best_serialize(data),
    );

    // TODO compressed postcard.
    print_results("Postcard", postcard_serialize(data));

    println!(
        "| ideal (max entropy)    |              | {:.2}%      |",
        100.0 / 255.0
    );
}

#[test]
fn comparison2() {
    fn compare<T: Serialize + Clone>(name: &str, r: RangeInclusive<T>) {
        fn measure<T: Serialize + Clone>(t: T) -> [usize; 6] {
            const COUNT: usize = 8;
            let many: [T; COUNT] = std::array::from_fn(|_| t.clone());
            [
                bitcode_default_serialize(&many).len(),
                bitcode_full_gamma_serialize(&many).len(),
                bitcode_bounded_serialize(&many).len(),
                bincode_fixint_serialize(&many).len(),
                bincode_varint_serialize(&many).len(),
                postcard_serialize(&many).len(),
            ]
            .map(|b| 8 * b / COUNT)
        }

        let lo = measure(r.start().clone());
        let hi = measure(r.end().clone());

        let v: Vec<_> = lo
            .into_iter()
            .zip(hi)
            .map(|(lo, hi)| {
                if lo == hi {
                    format!("{lo}")
                } else {
                    format!("{}-{}", usize::min(lo, hi), usize::max(lo, hi))
                }
            })
            .collect();
        println!(
            "| {name:<20} | {:<15} | {:<12} | {:<15} | {:<7} | {:<16} | {:<8} |",
            v[0], v[1], v[2], v[3], v[4], v[5],
        );
    }

    fn compare_one<T: Serialize + Clone>(name: &str, t: T) {
        compare(name, t.clone()..=t);
    }

    println!("| Type                 | Bitcode Default | Bitcode Full | Bitcode Bounded | Bincode | Bincode (Varint) | Postcard |");
    println!("|----------------------|-----------------|--------------|-----------------|---------|------------------|----------|");
    compare("bool", false..=true);
    compare("u8", 0u8..=u8::MAX);
    compare("u16", 0u16..=u16::MAX);
    // The FullGammaEncoding don't support the max values.
    compare("u32", 0u32..=u32::MAX - 1);
    compare("u64", 0u64..=u64::MAX - 1);
    // The worst case for signed integers are the negative values.
    compare("i8", (i8::MIN + 1)..=0i8);
    compare("i16", (i16::MIN + 1)..=0i16);
    compare("i32", (i32::MIN + 1)..=0i32);
    compare("i64", (i64::MIN + 1)..=0i64);
    compare_one("f32", 0f32);
    compare_one("f64", 0f64);
    compare("char", (0 as char)..=char::MAX);
    compare("Option<()>", None..=Some(()));
    compare("Result<(), ()>", Ok(())..=Err(()));

    println!();
    println!("| Type                 | Bitcode Default | Bitcode Full | Bitcode Bounded | Bincode | Bincode (Varint) | Postcard |");
    println!("|----------------------|-----------------|--------------|-----------------|---------|------------------|----------|");
    compare_one("[true; 4]", [true; 4]);
    compare_one("vec![(); 0]", vec![(); 0]);
    compare_one("vec![(); 1]", vec![(); 1]);
    compare_one("vec![(); 256]", vec![(); 256]);
    compare_one("vec![(); 65536]", vec![(); 65536]);
    compare_one("vec![1234u64; 0]", vec![1234u64; 0]);
    compare_one("vec![1234u64; 1]", vec![1234u64; 1]);
    compare_one("vec![1234u64; 256]", vec![1234u64; 256]);
    compare_one("vec![1234u64; 65536]", vec![1234u64; 65536]);
    compare_one(r#""""#, "");
    compare_one(r#""abcd""#, "abcd");
    compare_one(r#""abcd1234""#, "abcd1234");
}
