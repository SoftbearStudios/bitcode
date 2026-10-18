use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::variant::{VariantDecoder, VariantEncoder};
use crate::derive::{Decode, Encode};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ops::Bound;

pub struct BoundEncoder<T: Encode> {
    variants: VariantEncoder<u8, 3>,
    included: T::Encoder,
    excluded: T::Encoder,
}

// Can't derive since it would bound T: Default.
impl<T: Encode> Default for BoundEncoder<T> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            included: Default::default(),
            excluded: Default::default(),
        }
    }
}

impl<T: Encode> Encoder<Bound<T>> for BoundEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &Bound<T>) {
        let (variant, value) = match t {
            Bound::Included(t) => (0, Some((&mut self.included, t))),
            Bound::Excluded(t) => (1, Some((&mut self.excluded, t))),
            Bound::Unbounded => (2, None),
        };
        self.variants.encode(&variant);
        if let Some((encoder, value)) = value {
            encoder.reserve(NonZeroUsize::new(1).unwrap());
            encoder.encode(value);
        }
    }
}

impl<T: Encode> Buffer for BoundEncoder<T> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.variants.collect_into(out);
        self.included.collect_into(out);
        self.excluded.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.variants.reserve(additional);
        // We don't know how many bounds have each variant, so we can't reserve more.
    }
}

pub struct BoundDecoder<'a, T: Decode<'a>> {
    variants: VariantDecoder<'a, u8, 3, 3>,
    included: T::Decoder,
    excluded: T::Decoder,
}

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>> Default for BoundDecoder<'a, T> {
    fn default() -> Self {
        Self {
            variants: Default::default(),
            included: Default::default(),
            excluded: Default::default(),
        }
    }
}

impl<'a, T: Decode<'a>> View<'a> for BoundDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.variants.populate(input, length)?;
        self.included.populate(input, self.variants.length(0))?;
        self.excluded.populate(input, self.variants.length(1))
    }
}

impl<'a, T: Decode<'a>> Decoder<'a, Bound<T>> for BoundDecoder<'a, T> {
    #[inline(always)]
    fn decode_in_place(&mut self, out: &mut MaybeUninit<Bound<T>>) {
        out.write(match self.variants.decode() {
            0 => Bound::Included(self.included.decode()),
            1 => Bound::Excluded(self.excluded.decode()),
            _ => Bound::Unbounded,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};
    use alloc::string::String;

    #[test]
    fn test_bound_variants() {
        for value in [
            Bound::Included(0u32),
            Bound::Excluded(u32::MAX),
            Bound::Unbounded,
        ] {
            assert_eq!(decode::<Bound<u32>>(&encode(&value)).unwrap(), value);
        }
        assert_eq!(
            decode::<Vec<Bound<u8>>>(&encode(&Vec::<Bound<u8>>::new())).unwrap(),
            []
        );
    }

    #[test]
    fn test_bound_vectors_and_reused_buffer() {
        let values: Vec<_> = (0..300u32)
            .map(|i| match i % 3 {
                0 => Bound::Included(i),
                1 => Bound::Excluded(i),
                _ => Bound::Unbounded,
            })
            .collect();
        let mut buffer = crate::Buffer::new();
        for value in [
            &values,
            &vec![Bound::Unbounded],
            &vec![Bound::Included(7)],
            &values,
        ] {
            let bytes = buffer.encode(value).to_vec();
            assert_eq!(buffer.decode::<Vec<Bound<u32>>>(&bytes).unwrap(), *value);
        }
    }

    #[test]
    fn test_bound_borrowed_strings() {
        let values = vec![
            Bound::Included(String::from("start")),
            Bound::Excluded(String::from("end")),
            Bound::Unbounded,
        ];
        let bytes = encode(&values);
        let decoded = decode::<Vec<Bound<&str>>>(&bytes).unwrap();
        assert_eq!(
            decoded,
            [
                Bound::Included("start"),
                Bound::Excluded("end"),
                Bound::Unbounded
            ]
        );
        if let Bound::Included(s) = decoded[0] {
            assert!(bytes.as_ptr_range().contains(&s.as_ptr()));
        } else {
            panic!("missing included string");
        }
    }

    #[test]
    fn test_bound_invalid_input() {
        let invalid_char = encode(&Bound::Included(0x11_0000u32));
        assert!(decode::<Bound<char>>(&invalid_char).is_err());
        let mut trailing = encode(&Bound::<u32>::Unbounded);
        trailing.push(0);
        assert!(decode::<Bound<u32>>(&trailing).is_err());
        assert!(decode::<Bound<u32>>(&[]).is_err());
        for value in [Bound::Included(0x12345678u32), Bound::Excluded(0x87654321)] {
            let bytes = encode(&value);
            for end in 0..bytes.len() {
                assert!(decode::<Bound<u32>>(&bytes[..end]).is_err());
            }
        }
    }

    #[cfg(feature = "derive")]
    #[test]
    fn test_bound_matches_derived_enum_and_non_clone_payload() {
        #[derive(crate::Encode, crate::Decode, Debug, PartialEq)]
        enum Equivalent<T> {
            Included(T),
            Excluded(T),
            Unbounded,
        }
        #[derive(crate::Encode, crate::Decode, Debug, PartialEq)]
        struct Payload(String);
        let bounds = vec![
            Bound::Included(Payload(String::from("a"))),
            Bound::Excluded(Payload(String::from("b"))),
            Bound::Unbounded,
        ];
        let equivalent = vec![
            Equivalent::Included(Payload(String::from("a"))),
            Equivalent::Excluded(Payload(String::from("b"))),
            Equivalent::Unbounded,
        ];
        let bytes = encode(&bounds);
        assert_eq!(bytes, encode(&equivalent));
        assert_eq!(decode::<Vec<Bound<Payload>>>(&bytes).unwrap(), bounds);
    }
}
