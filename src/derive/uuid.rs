use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use core::num::NonZeroUsize;
use uuid::Uuid;

#[derive(Default)]
pub struct UuidEncoder(<u128 as Encode>::Encoder);

impl Encoder<Uuid> for UuidEncoder {
    #[inline(always)]
    fn encode(&mut self, t: &Uuid) {
        self.0.encode(&t.as_u128());
    }
}

impl Buffer for UuidEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

impl Encode for Uuid {
    type Encoder = UuidEncoder;
}

#[derive(Default)]
pub struct UuidDecoder<'a>(<u128 as Decode<'a>>::Decoder);

impl<'a> View<'a> for UuidDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)
    }
}

impl<'a> Decoder<'a, Uuid> for UuidDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> Uuid {
        Uuid::from_u128(self.0.decode())
    }
}

impl<'a> Decode<'a> for Uuid {
    type Decoder = UuidDecoder<'a>;
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use uuid::{uuid, Uuid};

    #[test]
    fn test() {
        assert!(crate::decode::<Uuid>(&crate::encode(&uuid!(
            "d1660702-561b-48e7-add0-c222143ca13c"
        )))
        .is_ok());
    }

    fn bench_data() -> Vec<Uuid> {
        crate::random_data(1000)
            .into_iter()
            .map(|n: u128| Uuid::from_u128(n))
            .collect()
    }
    crate::bench_encode_decode!(uuid_vec: Vec<_>);
}
