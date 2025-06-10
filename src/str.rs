use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::consume::consume_bytes;
use crate::derive::vec::VecEncoder;
use crate::error::err;
use crate::fast::{NextUnchecked, SliceImpl};
use crate::length::LengthDecoder;
use crate::u8_char::U8Char;
use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::num::NonZeroUsize;
use core::str::{from_utf8, from_utf8_unchecked};

#[derive(Default)]
pub struct StrEncoder(pub(crate) VecEncoder<U8Char>); // pub(crate) for arrayvec.rs

#[inline(always)]
fn str_as_u8_chars(s: &str) -> &[U8Char] {
    bytemuck::must_cast_slice(s.as_bytes())
}

impl Buffer for StrEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

impl Encoder<str> for StrEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &str) {
        self.0.encode(str_as_u8_chars(t));
    }

    #[inline(always)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a str> + Clone) {
        self.0.encode_vectored(i.map(str_as_u8_chars));
    }
}

// TODO find a way to remove this shim.
impl<'b> Encoder<&'b str> for StrEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &&str) {
        self.encode(*t);
    }

    #[inline(always)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a &'b str> + Clone)
    where
        &'b str: 'a,
    {
        self.encode_vectored(i.copied());
    }
}

impl Encoder<String> for StrEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &String) {
        self.encode(t.as_str());
    }

    #[inline(always)]
    fn encode_vectored<'a>(&mut self, i: impl Iterator<Item = &'a String> + Clone)
    where
        String: 'a,
    {
        self.encode_vectored(i.map(String::as_str));
    }
}

// Doesn't use VecDecoder because can't decode &[u8].
#[derive(Default)]
pub struct StrDecoder<'a> {
    // pub(crate) for arrayvec::ArrayString.
    pub(crate) lengths: LengthDecoder<'a>,
    strings: SliceImpl<'a, u8>,
}

impl<'a> View<'a> for StrDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.lengths.populate(input, length)?;
        let bytes = consume_bytes(input, self.lengths.length())?;

        // Fast path: If bytes are ASCII then they're valid UTF-8 and no char boundary can be invalid.
        // TODO(optimization):
        // - Worst case when bytes doesn't fit in CPU cache, this will load bytes 3 times from RAM.
        // - We should subdivide it into chunks in that case.
        if is_ascii_simd(bytes)
            || from_utf8(bytes).is_ok_and(|s| {
                // length == 0 implies bytes.is_empty() so no char boundaries can be broken. This
                // early exit allows us to do length.get() - 1 without possibility of overflow.
                let Some(length) = NonZeroUsize::new(length) else {
                    debug_assert_eq!(bytes.len(), 0);
                    return true;
                };
                // Check that gaps between individual strings are on char boundaries in larger string.
                // Boundaries at start and end of `s` aren't checked since s: &str guarantees them.
                let mut length_decoder = self.lengths.borrowed_clone();
                let mut end = 0;
                for _ in 0..length.get() - 1 {
                    end += length_decoder.decode();
                    // TODO(optimization) is_char_boundary has unnecessary checks.
                    if !s.is_char_boundary(end) {
                        return false;
                    }
                }
                true
            })
        {
            self.strings = bytes.into();
            Ok(())
        } else {
            err("invalid utf8")
        }
    }
}

impl<'a> Decoder<'a, &'a str> for StrDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> &'a str {
        let bytes = unsafe { self.strings.chunk_unchecked(self.lengths.decode()) };
        debug_assert!(from_utf8(bytes).is_ok());

        // Safety: `bytes` is valid UTF-8 because populate checked that `self.strings` is valid UTF-8
        // and that every sub string starts and ends on char boundaries.
        unsafe { from_utf8_unchecked(bytes) }
    }
}

impl<'a> Decoder<'a, String> for StrDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> String {
        let v: &str = self.decode();
        v.to_owned()
    }
}

/// Tests 128 bytes a time instead of `<[u8]>::is_ascii` which only tests 8.
/// 390% faster on 8KB, 27% faster on 1GB (RAM bottleneck).
fn is_ascii_simd(v: &[u8]) -> bool {
    const CHUNK: usize = 128;
    let chunks_exact = v.chunks_exact(CHUNK);
    let remainder = chunks_exact.remainder();
    for chunk in chunks_exact {
        let mut any = false;
        for &v in chunk {
            any |= v & 0x80 != 0;
        }
        if any {
            debug_assert!(!chunk.is_ascii());
            return false;
        }
    }
    debug_assert!(v[..v.len() - remainder.len()].is_ascii());
    remainder.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::is_ascii_simd;
    use crate::u8_char::U8Char;
    use crate::{decode, encode};
    use alloc::borrow::ToOwned;
    use test::{black_box, Bencher};

    #[test]
    fn utf8_validation() {
        // Check from_utf8:
        assert!(decode::<&str>(&encode(&vec![U8Char(255u8)])).is_err());
        assert_eq!(decode::<&str>(&encode("\0")).unwrap(), "\0");
        assert_eq!(decode::<&str>(&encode(&"☺".to_owned())).unwrap(), "☺");

        let c = "☺";
        let full = super::str_as_u8_chars(c);
        let start = &full[..1];
        let end = &full[1..];

        // Check is_char_boundary:
        assert!(decode::<[&str; 2]>(&encode(&[start.to_vec(), end.to_vec()])).is_err());
        assert_eq!(decode::<[&str; 2]>(&encode(&[c, c])).unwrap(), [c, c]);
    }

    #[test]
    fn test_is_ascii_simd() {
        assert!(is_ascii_simd(&[0x7F; 128]));
        assert!(!is_ascii_simd(&[0xFF; 128]));
    }

    #[bench]
    fn bench_is_ascii(b: &mut Bencher) {
        b.iter(|| black_box(&[0; 8192]).is_ascii())
    }

    #[bench]
    fn bench_is_ascii_simd(b: &mut Bencher) {
        b.iter(|| is_ascii_simd(black_box(&[0; 8192])))
    }

    type S = &'static str;
    fn bench_data() -> (S, S, S, S, S, S, S) {
        ("a", "b", "c", "d", "e", "f", "g")
    }
    crate::bench_encode_decode!(str_tuple: (&str, &str, &str, &str, &str, &str, &str));

    // Don't do this in miri since it leaks memory.
    #[test]
    #[cfg(all(feature = "derive", not(miri)))]
    fn decode_static_from_static_buffer() {
        #[derive(crate::Encode, crate::Decode)]
        struct Test {
            text: &'static str,
        }

        let _var = {
            let bytes = encode(&Test { text: "hi" }).leak();
            decode::<Test>(&*bytes).unwrap()
        };
    }

    #[test]
    #[cfg(feature = "derive")]
    fn decode_phantom_static_from_non_static_buffer() {
        use core::marker::PhantomData;

        #[derive(crate::Encode, crate::Decode)]
        struct Test {
            text: PhantomData<&'static str>,
        }

        let _var = {
            let bytes = encode(&Test { text: PhantomData });
            decode::<Test>(&*bytes).unwrap()
        };
    }
}

/// ```compile_fail,E0597
/// use bitcode::{encode, decode, Encode, Decode};
///
/// #[derive(Decode)]
/// struct Test {
///     text: &'static str,
/// }
///
/// let _var = {
///     let var = [0];
///     decode::<Test>(&var).unwrap()
/// };
/// ```
#[doc(hidden)]
pub fn _cant_decode_static_from_non_static_buffer() {}

/// ```compile_fail,E0495
/// use bitcode::{encode, decode, Encode, Decode};
///
/// type StaticStr = &'static str;
///
/// #[derive(Decode)]
/// struct Test {
///     text: StaticStr,
/// }
///
///
/// let _var = {
///     decode::<Test>(&[]).unwrap()
/// };
/// ```
#[doc(hidden)]
pub fn _cant_decode_static_alias_at_all() {}

#[cfg(test)]
mod tests2 {
    use alloc::string::String;
    use alloc::vec::Vec;

    fn bench_data() -> Vec<String> {
        crate::random_data::<u8>(40000)
            .into_iter()
            .map(|n| {
                let n = (8 + n / 32) as usize;
                " ".repeat(n)
            })
            .collect()
    }
    crate::bench_encode_decode!(str_vec: Vec<String>);
}
