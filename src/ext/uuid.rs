use uuid::Uuid;
use crate::derive::convert::ConvertFrom;
use crate::convert::impl_convert;

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

impl_convert!(uuid::Uuid, UuidConversion);

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use uuid::Uuid;

    #[test]
    fn test() {
        assert!(crate::decode::<Uuid>(&crate::encode(&Uuid::new_v4())).is_ok());
    }

    fn bench_data() -> Vec<Uuid> {
        crate::random_data(1000)
            .into_iter()
            .map(|n: super::UuidConversion| Uuid::from_bytes(n))
            .collect()
    }
    crate::bench_encode_decode!(uuid_vec: Vec<Uuid>);
}
