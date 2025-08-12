use uuid::Uuid;
use crate::derive::convert::ConvertFrom;
use crate::convert::impl_convert;

impl ConvertFrom<&Uuid> for u128 {
    fn convert_from(value: &Uuid) -> Self {
        value.as_u128()
    }
}

impl ConvertFrom<u128> for Uuid {
    fn convert_from(value: u128) -> Self {
        Uuid::from_u128(value)
    }
}

impl_convert!(uuid::Uuid, u128);

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
            .map(|n: u128| Uuid::from_u128(n))
            .collect()
    }
    crate::bench_encode_decode!(uuid_vec: Vec<Uuid>);
}
