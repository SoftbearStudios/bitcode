use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use std::marker::PhantomData;
use std::num::NonZeroUsize;

#[derive(Debug, Default)]
pub struct EmptyCoder;

impl<T> Encoder<PhantomData<T>> for EmptyCoder {
    fn encode(&mut self, _: &PhantomData<T>) {}
}

impl Buffer for EmptyCoder {
    fn collect_into(&mut self, _: &mut Vec<u8>) {}
    fn reserve(&mut self, _: NonZeroUsize) {}
}

impl<'a> View<'a> for EmptyCoder {
    fn populate(&mut self, _: &mut &'a [u8], _: usize) -> Result<()> {
        Ok(())
    }
}

impl<'a, T> Decoder<'a, PhantomData<T>> for EmptyCoder {
    fn decode(&mut self) -> PhantomData<T> {
        PhantomData
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;
    fn bench_data() -> Vec<PhantomData<()>> {
        vec![PhantomData; 100]
    }
    crate::bench_encode_decode!(phantom_data_vec: Vec<_>);
}
