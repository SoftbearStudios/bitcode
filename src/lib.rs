#![allow(clippy::items_after_test_module, clippy::blocks_in_if_conditions)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, feature(test))]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

#[macro_use]
extern crate alloc;

// Fixes derive macro in tests/doc tests.
#[cfg(test)]
extern crate self as bitcode;
#[cfg(test)]
extern crate test;

mod bool;
mod buffer;
mod coder;
mod consume;
mod derive;
mod error;
mod ext;
mod f32;
mod fast;
mod histogram;
mod int;
mod length;
mod nightly;
mod pack;
mod pack_ints;
mod str;
mod u8_char;

pub use crate::buffer::Buffer;
pub use crate::derive::*;
pub use crate::error::Error;

#[cfg(feature = "derive")]
pub use bitcode_derive::{Decode, Encode};

#[cfg(feature = "serde")]
mod serde;
#[cfg(feature = "serde")]
pub use crate::serde::*;

#[cfg(test)]
mod benches;
#[cfg(test)]
mod benches_borrowed;

#[cfg(test)]
use alloc::vec::Vec;

#[cfg(test)]
fn random_data<T>(n: usize) -> Vec<T>
where
    rand::distributions::Standard: rand::distributions::Distribution<T>,
{
    let n = limit_bench_miri(n);
    use rand::prelude::*;
    let mut rng = rand_chacha::ChaCha20Rng::from_seed(Default::default());
    (0..n).map(|_| rng.gen()).collect()
}
#[cfg(test)]
fn limit_bench_miri(n: usize) -> usize {
    if cfg!(miri) {
        (n / 100).max(10).min(1000)
    } else {
        n
    }
}
#[cfg(test)]
macro_rules! bench_encode_decode {
    ($($name:ident: $t:ty),+) => {
        paste::paste! {
            $(
                #[bench]
                fn [<bench_ $name _encode>](b: &mut test::Bencher) {
                    let data: $t = bench_data();
                    let mut buffer = crate::Buffer::default();
                    b.iter(|| {
                        test::black_box(buffer.encode(test::black_box(&data)));
                    })
                }

                #[bench]
                fn [<bench_ $name _decode>](b: &mut test::Bencher) {
                    let data: $t = bench_data();
                    let encoded = crate::encode(&data);
                    let mut buffer = crate::Buffer::default();

                    // Help type inference.
                    fn check_eof<'a, T: crate::Decode<'a>>(_: &T, incomplete: &'a [u8], padded: &'a [u8]) {
                        let Err(e) = crate::decode::<T>(incomplete) else {
                            panic!("no eof");
                        };
                        assert_eq!(e, crate::error::error("EOF"));

                        let Err(e) = crate::decode::<T>(&padded) else {
                            panic!("no expected eof");
                        };
                        assert_eq!(e, crate::error::error("Expected EOF"));
                    }
                    {
                        let mut padded = encoded.to_vec();
                        padded.push(0);
                        check_eof(&data, &encoded[..encoded.len() - 1], &padded);
                    }

                    let mut f = || {
                        #[cfg(miri)] // Make sure dangling pointers aren't read due to Buffer.
                        let encoded = encoded.clone();

                        let decoded: $t = buffer.decode(test::black_box(&encoded)).unwrap();
                        debug_assert_eq!(data, decoded);
                        test::black_box(decoded);
                    };

                    // Make sure f gets called at least twice (b.iter() calls once with miri).
                    if cfg!(miri) {
                        f();
                        f();
                    } else {
                        b.iter(f);
                    }
                }
            )+
        }
    }
}
#[cfg(test)]
pub(crate) use bench_encode_decode;
