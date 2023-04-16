#![cfg_attr(test, feature(test))]
#![forbid(unsafe_code)]

//! Bitcode is a crate for encoding and decoding using a tinier
//! binary serialization strategy. You can easily go from having
//! an object in memory, quickly serialize it to bytes, and then
//! deserialize it back just as fast!
//!
//! The format is not necessarily stable between versions. If you want
//! a stable format, consider [bincode](https://docs.rs/bincode/latest/bincode/).
//!
//! ### Usage
//!
//! ```edition2021
//! // The object that we will serialize.
//! let target: Vec<String> = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
//!
//! let encoded: Vec<u8> = bitcode::serialize(&target).unwrap();
//! let decoded: Vec<String> = bitcode::deserialize(&encoded).unwrap();
//! assert_eq!(target, decoded);
//! ```

// Actually required see https://doc.rust-lang.org/beta/unstable-book/library-features/test.html
#[cfg(test)]
extern crate core;
#[cfg(test)]
extern crate test;

use de::{deserialize_with, read::ReadWithImpl};
use ser::{serialize_with, write::WriteWithImpl};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

mod de;
mod nightly;
mod ser;
#[cfg(test)]
mod tests;

/// Serializes a `T:` [`Serialize`] into a [`Vec<u8>`].
///
/// **Warning:** The format is subject to change between versions.
pub fn serialize<T: ?Sized>(t: &T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    serialize_with::<WriteWithImpl>(t)
}

/// Deserializes a [`&[u8]`][`prim@slice`] into an instance of `T:` [`Deserialize`].
///
/// **Warning:** The format is subject to change between versions.
pub fn deserialize<'a, T>(bytes: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    deserialize_with::<'a, T, ReadWithImpl>(bytes)
}

/// (De)serialization errors.
///
/// # Debug mode
///
/// In debug mode, the error contains a reason.
///
/// # Release mode
///
/// In release mode, the error is a zero-sized type for efficiency.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Error(ErrorImpl);

#[cfg(not(debug_assertions))]
type ErrorImpl = ();

#[cfg(debug_assertions)]
type ErrorImpl = E;

impl Error {
    /// Replaces an invalid message. E.g. read_variant_index calls read_len but converts
    /// `E::Invalid("length")` to `E::Invalid("variant index")`.
    pub(crate) fn map_invalid(self, _s: &'static str) -> Self {
        #[cfg(debug_assertions)]
        return Self(match self.0 {
            E::Invalid(_) => E::Invalid(_s),
            _ => self.0,
        });
        #[cfg(not(debug_assertions))]
        self
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Inner error that can be converted to [`Error`] with [`E::e`].
#[derive(Debug, PartialEq)]
pub(crate) enum E {
    #[cfg(debug_assertions)]
    Custom(String),
    Eof,
    ExpectedEof,
    Invalid(&'static str),
    NotSupported(&'static str),
}

impl E {
    fn e(self) -> Error {
        #[cfg(debug_assertions)]
        return Error(self);
        #[cfg(not(debug_assertions))]
        Error(())
    }
}

type Result<T> = std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        return Display::fmt(&self.0, f);
        #[cfg(not(debug_assertions))]
        f.write_str("bitcode error")
    }
}

impl Display for E {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(debug_assertions)]
            Self::Custom(s) => write!(f, "custom: {s}"),
            Self::Eof => write!(f, "eof"),
            Self::ExpectedEof => write!(f, "expected eof"),
            Self::Invalid(s) => write!(f, "invalid {s}"),
            Self::NotSupported(s) => write!(f, "{s} is not supported"),
        }
    }
}

impl std::error::Error for Error {}

impl serde::ser::Error for Error {
    fn custom<T>(_msg: T) -> Self
    where
        T: Display,
    {
        #[cfg(debug_assertions)]
        return Self(E::Custom(_msg.to_string()));
        #[cfg(not(debug_assertions))]
        Self(())
    }
}

impl serde::de::Error for Error {
    fn custom<T>(_msg: T) -> Self
    where
        T: Display,
    {
        #[cfg(debug_assertions)]
        return Self(E::Custom(_msg.to_string()));
        #[cfg(not(debug_assertions))]
        Self(())
    }
}

#[cfg(test)]
mod tests2 {
    use bincode::Options;
    use flate2::read::DeflateDecoder;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
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

    #[inline(never)]
    fn bitcode_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
        super::serialize(v).unwrap()
    }

    #[inline(never)]
    fn bitcode_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
        super::deserialize(v).unwrap()
    }

    #[inline(never)]
    fn bincode_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
        bincode::serialize(v).unwrap()
    }

    #[inline(never)]
    fn bincode_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
        bincode::deserialize(v).unwrap()
    }

    #[inline(never)]
    fn bincode_varint_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
        bincode::DefaultOptions::new().serialize(v).unwrap()
    }

    #[inline(never)]
    fn bincode_varint_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
        bincode::DefaultOptions::new().deserialize(v).unwrap()
    }

    #[inline(never)]
    fn bincode_flate2_fast_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
        bincode::DefaultOptions::new()
            .serialize_into(&mut e, v)
            .unwrap();
        e.finish().unwrap()
    }

    #[inline(never)]
    fn bincode_flate2_best_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
        bincode::DefaultOptions::new()
            .serialize_into(&mut e, v)
            .unwrap();
        e.finish().unwrap()
    }

    #[inline(never)]
    fn bincode_flate2_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
        bincode::DefaultOptions::new()
            .deserialize_from(DeflateDecoder::new(v))
            .unwrap()
    }

    #[test]
    fn deserialize() {
        let data = vec![Data {
            x: None,
            y: None,
            z: 1,
            s: "a".into(),
            e: DataEnum::Bar,
        }];

        let v: Vec<Data> = bitcode_deserialize(&bitcode_serialize(&data));
        assert_eq!(v, data);
    }

    fn bench_data() -> Vec<Data> {
        random_data(1000)
    }

    fn bench_serialize_string(b: &mut Bencher, ser: fn(&str) -> Vec<u8>, bytes: usize) {
        let data = "a".repeat(bytes);
        b.iter(|| {
            black_box(ser(black_box(data.as_str())));
        })
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

    #[bench]
    fn bench_bitcode_small(b: &mut Bencher) {
        bench_serialize_string(b, bitcode_serialize, 5)
    }

    #[bench]
    fn bench_bincode_small(b: &mut Bencher) {
        bench_serialize_string(b, bincode_serialize, 5)
    }

    #[bench]
    fn bench_bitcode_large(b: &mut Bencher) {
        bench_serialize_string(b, bitcode_serialize, 10000)
    }

    #[bench]
    fn bench_bincode_large(b: &mut Bencher) {
        bench_serialize_string(b, bincode_serialize, 10000)
    }

    #[bench]
    fn bench_bitcode_serialize(b: &mut Bencher) {
        bench_serialize(b, bitcode_serialize)
    }

    #[bench]
    fn bench_bitcode_deserialize(b: &mut Bencher) {
        bench_deserialize(b, bitcode_serialize, bitcode_deserialize)
    }

    #[bench]
    fn bench_bincode_serialize(b: &mut Bencher) {
        bench_serialize(b, bincode_serialize)
    }

    #[bench]
    fn bench_bincode_deserialize(b: &mut Bencher) {
        bench_deserialize(b, bincode_serialize, bincode_deserialize)
    }

    #[bench]
    fn bench_bincode_varint_serialize(b: &mut Bencher) {
        bench_serialize(b, bincode_varint_serialize)
    }

    #[bench]
    fn bench_bincode_varint_deserialize(b: &mut Bencher) {
        bench_deserialize(b, bincode_varint_serialize, bincode_varint_deserialize)
    }

    #[bench]
    fn bench_bincode_flate2_fast_serialize(b: &mut Bencher) {
        bench_serialize(b, bincode_flate2_fast_serialize)
    }

    #[bench]
    fn bench_bincode_flate2_fast_deserialize(b: &mut Bencher) {
        bench_deserialize(b, bincode_flate2_fast_serialize, bincode_flate2_deserialize)
    }

    #[bench]
    fn bench_bincode_flate2_best_serialize(b: &mut Bencher) {
        bench_serialize(b, bincode_flate2_best_serialize)
    }

    #[bench]
    fn bench_bincode_flate2_best_deserialize(b: &mut Bencher) {
        bench_deserialize(b, bincode_flate2_best_serialize, bincode_flate2_deserialize)
    }

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
        print_results("Bitcode", bitcode_serialize(data));
        print_results("Bincode", bincode_serialize(data));
        print_results("Bincode (Varint)", bincode_varint_serialize(data));

        // These use varint since it makes the result smaller for low cost compared to flate2.
        print_results(
            "Bincode (Deflate Fast)",
            bincode_flate2_fast_serialize(data),
        );
        print_results(
            "Bincode (Deflate Best)",
            bincode_flate2_best_serialize(data),
        );

        println!(
            "| ideal (max entropy)    |              | {:.2}%      |",
            100.0 / 255.0
        );
    }

    #[test]
    fn comparison2() {
        fn compare<T: Serialize + Clone>(name: &str, r: RangeInclusive<T>) {
            fn measure<T: Serialize + Clone>(t: T) -> [usize; 3] {
                const COUNT: usize = 8;
                let many: [T; COUNT] = std::array::from_fn(|_| t.clone());
                let bitcode = 8 * bitcode_serialize(&many).len() / COUNT;
                let bincode = 8 * bincode_serialize(&many).len() / COUNT;
                let bincode_varint = 8 * bincode_varint_serialize(&many).len() / COUNT;
                [bitcode, bincode, bincode_varint]
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
                        format!("{lo}-{hi}")
                    }
                })
                .collect();
            println!("| {name:<15} | {:<7} | {:<7} | {:<16} |", v[0], v[1], v[2]);
        }

        fn compare_one<T: Serialize + Clone>(name: &str, t: T) {
            compare(name, t.clone()..=t);
        }

        println!("| Type            | Bitcode | Bincode | Bincode (Varint) |");
        println!("|-----------------|---------|---------|------------------|");
        compare("bool", false..=true);
        compare("u8", 0u8..=u8::MAX);
        compare("i8", 0i8..=i8::MAX);
        compare("u16", 0u16..=u16::MAX);
        compare("i16", 0i16..=i16::MAX);
        compare("u32", 0u32..=u32::MAX);
        compare("i32", 0i32..=i32::MAX);
        compare("u64", 0u64..=u64::MAX);
        compare("i64", 0i64..=i64::MAX);
        compare_one("f32", 0f32);
        compare_one("f64", 0f64);
        compare("char", (0 as char)..=char::MAX);
        compare("Option<()>", None..=Some(()));
        compare("Result<(), ()>", Ok(())..=Err(()));

        println!();
        println!("| Value           | Bitcode | Bincode | Bincode (Varint) |");
        println!("|-----------------|---------|---------|------------------|");
        compare_one("[true; 4]", [true; 4]);
        compare_one("vec![(); 0]", vec![(); 0]);
        compare_one("vec![(); 1]", vec![(); 1]);
        compare_one("vec![(); 256]", vec![(); 256]);
        compare_one("vec![(); 65536]", vec![(); 65536]);
        compare_one(r#""""#, "");
        compare_one(r#""abcd""#, "abcd");
        compare_one(r#""abcd1234""#, "abcd1234");
    }
}
