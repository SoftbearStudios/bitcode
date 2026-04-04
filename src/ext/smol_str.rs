use crate::derive::smart_ptr::{DerefEncoder, FromDecoder};
use crate::derive::{Decode, Encode};
use smol_str::SmolStr;

/// Encodes `SmolStr` as `&str` via `Deref` (zero-copy, no allocation).
/// Decodes `&str` borrowed from the buffer, then converts via `From<&str>` for SmolStr.
/// Short strings (≤23 bytes) are stored inline without heap allocation.
impl Encode for SmolStr {
    type Encoder = DerefEncoder<str>;
}

impl<'a> Decode<'a> for SmolStr {
    type Decoder = FromDecoder<'a, &'a str>;
}

#[cfg(test)]
mod tests {
    use smol_str::SmolStr;

    /// Short strings stay inline after decode (no heap allocation).
    #[test]
    fn decoded_short_string_is_not_heap_allocated() {
        let s = SmolStr::new("hello");
        assert!(!s.is_heap_allocated());
        let decoded: SmolStr = crate::decode(&crate::encode(&s)).unwrap();
        assert!(!decoded.is_heap_allocated());
    }

    /// Long strings are heap-allocated after decode.
    #[test]
    fn decoded_long_string_is_heap_allocated() {
        let s = SmolStr::new("this is a longer string that exceeds inline storage");
        assert!(s.is_heap_allocated());
        let decoded: SmolStr = crate::decode(&crate::encode(&s)).unwrap();
        assert!(decoded.is_heap_allocated());
    }
}
