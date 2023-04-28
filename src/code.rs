use crate::encoding::{Encoding, Fixed};
use crate::read::Read;
use crate::write::Write;
use crate::Result;

pub(crate) fn encode_internal<'a>(
    writer: &'a mut (impl Write + Default),
    t: &(impl Encode + ?Sized),
) -> Result<&'a [u8]> {
    writer.start_write();
    t.encode(Fixed, writer)?;
    Ok(writer.finish_write())
}

pub(crate) fn decode_internal<'a, T: Decode>(
    reader: &mut (impl Read + Default),
    bytes: &[u8],
) -> Result<T> {
    reader.start_read(bytes);
    let decode_result = T::decode(Fixed, reader);
    reader.finish_read_with_result(decode_result)
}

/// A type which can be encoded to bytes with [`encode`][`crate::encode`].
///
/// Must use `#[derive(Encode)]` to implement.
/// ```
/// #[derive(bitcode::Encode)]
/// // If your struct contains itself you must annotate it with `#[bitcode(recursive)]`.
/// // This disables certain speed optimizations that aren't possible on recursive types.
/// struct MyStruct {
///     a: u32,
///     b: bool,
///     // If you want to use serde::Serialize on a field instead of bitcode::Encode.
///     #[cfg(feature = "serde")]
///     #[bitcode(with_serde)]
///     c: String,
/// }
/// ```
pub trait Encode {
    // TODO make these const functions that take an encoding (once const fn is available in traits).
    // For now these are only valid if encoding is fixed. Before using them make sure the encoding
    // passed to encode is fixed.
    #[doc(hidden)]
    const MIN_BITS: usize;
    #[doc(hidden)]
    const MAX_BITS: usize;

    #[doc(hidden)]
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()>;
}

/// A type which can be decoded from bytes with [`decode`][`crate::decode`].
///
/// Must use `#[derive(Decode)]` to implement.
/// ```
/// #[derive(bitcode::Decode)]
/// // If your struct contains itself you must annotate it with `#[bitcode(recursive)]`.
/// // This disables certain speed optimizations that aren't possible on recursive types.
/// struct MyStruct {
///     a: u32,
///     b: bool,
///     // If you want to use serde::Deserialize on a field instead of bitcode::Decode.
///     #[cfg(feature = "serde")]
///     #[bitcode(with_serde)]
///     c: String,
/// }
/// ```
pub trait Decode: Sized {
    #[doc(hidden)]
    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self>;
}

/// A macro that facilitates writing to a RegisterBuffer when encoding multiple values less than 64 bits.
/// This can dramatically speed operations like encoding a tuple of 8 bytes.
///
/// Once you call `optimized_enc!()`, you must call `end_enc!()` at the end to flush the remaining bits.
#[doc(hidden)]
#[macro_export]
macro_rules! optimized_enc {
    ($encoding:ident, $writer:ident) => {
        // Put __ in front of fields just in case proc macro shadows them.
        // TODO use canonical names in proc macro.
        let mut __buf = $crate::__private::RegisterBuffer::default();
        #[allow(unused_mut)]
        let mut __i: usize = 0;
        #[allow(unused)]
        let __no_encoding_upstream = $encoding.is_fixed();

        // Call on each field (that doesn't get it's encoding overridden in the derive macro).
        #[allow(unused)]
        macro_rules! enc {
            ($t:expr, $T:ty) => {
                // MAX_BITS is only accurate if there isn't any encoding upstream.
                // Downstream encodings make MAX_BITS = usize::MAX in derive macro.
                if <$T>::MAX_BITS.saturating_add(__i) <= 64 && __no_encoding_upstream {
                    <$T>::encode(&$t, $encoding, &mut __buf)?;
                } else {
                    if __i != 0 {
                        __buf.flush($writer);
                    }

                    if <$T>::MAX_BITS < 64 && __no_encoding_upstream {
                        <$T>::encode(&$t, $encoding, &mut __buf)?;
                    } else {
                        <$T>::encode(&$t, $encoding, $writer)?;
                    }
                }

                __i = if <$T>::MAX_BITS.saturating_add(__i) <= 64 && __no_encoding_upstream {
                    <$T>::MAX_BITS + __i
                } else {
                    if <$T>::MAX_BITS < 64 && __no_encoding_upstream {
                        <$T>::MAX_BITS
                    } else {
                        0
                    }
                };
            };
        }

        // Call before you write anything to writer after you have called enc!.
        macro_rules! flush {
            () => {
                if __i != 0 {
                    __buf.flush($writer);
                }
            };
        }

        // Call once done encoding.
        macro_rules! end_enc {
            () => {
                flush!();
            };
        }
    };
}
pub use optimized_enc;

// These benchmarks ensure that optimized_enc is working. They all run about 8 times faster with optimized_enc.
#[cfg(test)]
mod optimized_enc_tests {
    use test::{black_box, Bencher};

    type A = u8;
    type B = u8;

    // TODO remove.
    #[derive(Clone, Debug, PartialEq, crate::Encode, crate::Decode)]
    struct Foo {
        a: A,
        b: B,
    }

    #[bench]
    fn bench_foo(b: &mut Bencher) {
        let mut buffer = crate::Buffer::new();
        let foo = Foo { a: 1, b: 2 };
        let foo = vec![foo; 4000];

        let bytes = buffer.encode(&foo).unwrap().to_vec();
        let decoded: Vec<Foo> = buffer.decode(&bytes).unwrap();
        assert_eq!(foo, decoded);

        b.iter(|| {
            let foo = black_box(foo.as_slice());
            let bytes = buffer.encode(foo).unwrap();
            black_box(bytes);
        })
    }

    #[bench]
    fn bench_tuple(b: &mut Bencher) {
        let mut buffer = crate::Buffer::new();
        let foo = vec![(0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8); 1000];

        b.iter(|| {
            let foo = black_box(foo.as_slice());
            let bytes = buffer.encode(foo).unwrap();
            black_box(bytes);
        })
    }

    #[bench]
    fn bench_array(b: &mut Bencher) {
        let mut buffer = crate::Buffer::new();
        let foo = vec![[0u8; 8]; 1000];

        b.iter(|| {
            let foo = black_box(foo.as_slice());
            let bytes = buffer.encode(foo).unwrap();
            black_box(bytes);
        })
    }

    #[bench]
    fn bench_byte_slice(b: &mut Bencher) {
        let mut buffer = crate::Buffer::new();
        let foo = vec![0u8; 8 * 1000];

        b.iter(|| {
            let foo = black_box(foo.as_slice());
            let bytes = buffer.encode(foo).unwrap();
            black_box(bytes);
        })
    }

    #[bench]
    fn bench_bool_slice(b: &mut Bencher) {
        let mut buffer = crate::Buffer::new();
        let foo = vec![false; 8 * 1000];

        b.iter(|| {
            let foo = black_box(foo.as_slice());
            let bytes = buffer.encode(foo).unwrap();
            black_box(bytes);
        })
    }
}
