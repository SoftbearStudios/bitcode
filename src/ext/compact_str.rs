use crate::coder::{Decoder, Encoder};
use crate::derive::{Decode, Encode};
use crate::str::{StrDecoder, StrEncoder};
use compact_str::CompactString;

impl Encoder<CompactString> for StrEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &CompactString) {
        self.encode(t.as_str())
    }
}

impl Encode for CompactString {
    type Encoder = StrEncoder;
}

impl<'a> Decoder<'a, CompactString> for StrDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> CompactString {
        let s: &str = self.decode();
        s.into()
    }
}

impl<'a> Decode<'a> for CompactString {
    type Decoder = StrDecoder<'a>;
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use alloc::vec::Vec;
    use compact_str::CompactString;

    #[test]
    fn compact_string() {
        let mut v = CompactString::default();
        v.push('0');
        v.push('1');
        let b = encode(&v);
        assert_eq!(decode::<CompactString>(&b).unwrap(), v);
    }

    #[test]
    fn compact_string_long() {
        // CompactString inlines up to 23 bytes on 64-bit platforms; the English alphabet exceeds that at 26 bytes.
        let v = CompactString::new("abcdefghijklmnopqrstuvwxyz");
        let b = encode(&v);
        assert_eq!(decode::<CompactString>(&b).unwrap(), v);
    }

    #[test]
    fn compact_string_vec() {
        let v = vec![
            CompactString::new("01"),
            CompactString::new("abcdefghijklmnopqrstuvwxyz"),
        ];
        let b = encode(&v);
        let v2 = decode::<Vec<CompactString>>(&b).unwrap();
        assert_eq!(v, v2);
    }
}
