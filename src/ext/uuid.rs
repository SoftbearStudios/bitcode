use crate::convert::impl_convert;
use crate::derive::convert::ConvertFrom;
use uuid::Uuid;

type UuidConversion = [u8; 16];

impl ConvertFrom<&Uuid> for UuidConversion {
    fn convert_from(value: &Uuid) -> Self {
        value.into_bytes()
    }
}

impl ConvertFrom<UuidConversion> for Uuid {
    fn convert_from(value: UuidConversion) -> Self {
        Uuid::from_bytes(value)
    }
}

impl_convert!(Uuid, UuidConversion);

#[cfg(test)]
mod tests {
    use core::str::FromStr;
    use uuid::Uuid;

    #[test]
    fn roundtrip() {
        let uuid = Uuid::from_str("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8").unwrap();
        assert_eq!(crate::decode::<Uuid>(&crate::encode(&uuid)).unwrap(), uuid);
    }

    // By running this test on architectures with different endianness,
    // we ensure our implementation is endianness-invariant.
    #[test]
    fn endianness_invariance() {
        assert_eq!(
            crate::encode(&Uuid::from_str("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8").unwrap()),
            [0, 161, 162, 163, 164, 177, 178, 193, 194, 209, 210, 211, 212, 213, 214, 215, 216]
        )
    }
}
