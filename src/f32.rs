use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::consume::consume_byte_arrays;
use crate::fast::{FastSlice, NextUnchecked, PushUnchecked, VecImpl};
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;

#[derive(Debug, Default)]
pub struct F32Encoder(VecImpl<f32>);

impl Encoder<f32> for F32Encoder {
    #[inline(always)]
    fn encode(&mut self, t: &f32) {
        unsafe { self.0.push_unchecked(*t) };
    }
}

/// [`bytemuck`] doesn't implement [`MaybeUninit`] casts. Slightly different from
/// [`bytemuck::cast_slice_mut`] in that it will truncate partial elements instead of panicking.
fn chunks_uninit<A, B>(m: &mut [MaybeUninit<A>]) -> &mut [MaybeUninit<B>] {
    use std::mem::{align_of, size_of};
    assert_eq!(align_of::<B>(), align_of::<A>());
    assert_eq!(0, size_of::<B>() % size_of::<A>());
    let divisor = size_of::<B>() / size_of::<A>();
    // Safety: `align_of<B> == align_of<A>` and `size_of<B>()` is a multiple of `size_of<A>()`
    unsafe {
        std::slice::from_raw_parts_mut(m.as_mut_ptr() as *mut MaybeUninit<B>, m.len() / divisor)
    }
}

impl Buffer for F32Encoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        let floats = self.0.as_slice();
        let byte_len = std::mem::size_of_val(floats);
        out.reserve(byte_len);
        let uninit = &mut out.spare_capacity_mut()[..byte_len];

        let (mantissa, sign_exp) = uninit.split_at_mut(floats.len() * 3);
        let mantissa: &mut [MaybeUninit<[u8; 3]>] = chunks_uninit(mantissa);

        // TODO SIMD version with PSHUFB.
        const CHUNK_SIZE: usize = 4;
        let chunks_len = floats.len() / CHUNK_SIZE;
        let chunks_floats = chunks_len * CHUNK_SIZE;
        let chunks: &[[u32; CHUNK_SIZE]] = bytemuck::cast_slice(&floats[..chunks_floats]);
        let mantissa_chunks: &mut [MaybeUninit<[[u8; 4]; 3]>] = chunks_uninit(mantissa);
        let sign_exp_chunks: &mut [MaybeUninit<[u8; 4]>] = chunks_uninit(sign_exp);

        for ci in 0..chunks_len {
            let [a, b, c, d] = chunks[ci];

            let m0 = a & 0xFF_FF_FF | (b << 24);
            let m1 = ((b >> 8) & 0xFF_FF) | (c << 16);
            let m2 = (c >> 16) & 0xFF | (d << 8);
            let mantissa_chunk = &mut mantissa_chunks[ci];
            mantissa_chunk.write([m0.to_le_bytes(), m1.to_le_bytes(), m2.to_le_bytes()]);

            let se = (a >> 24) | ((b >> 24) << 8) | ((c >> 24) << 16) | ((d >> 24) << 24);
            let sign_exp_chunk = &mut sign_exp_chunks[ci];
            sign_exp_chunk.write(se.to_le_bytes());
        }

        for i in chunks_floats..floats.len() {
            let [m @ .., se] = floats[i].to_le_bytes();
            mantissa[i].write(m);
            sign_exp[i].write(se);
        }

        // Safety: We just initialized these elements in the loops above.
        unsafe { out.set_len(out.len() + byte_len) };
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}

#[derive(Debug, Default)]
pub struct F32Decoder<'a> {
    // While it is true that this contains 1 bit of the exp we still call it mantissa.
    mantissa: FastSlice<'a, [u8; 3]>,
    sign_exp: FastSlice<'a, u8>,
}

impl<'a> View<'a> for F32Decoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        let total: &[u8] = bytemuck::must_cast_slice(consume_byte_arrays::<4>(input, length)?);
        let (mantissa, sign_exp) = total.split_at(length * 3);
        let mantissa: &[[u8; 3]] = bytemuck::cast_slice(mantissa);
        // Equivalent to `mantissa.into()` but satisfies miri when we read extra in decode.
        self.mantissa =
            unsafe { FastSlice::from_raw_parts(total.as_ptr() as *const [u8; 3], mantissa.len()) };
        self.sign_exp = sign_exp.into();
        Ok(())
    }
}

impl<'a> Decoder<'a, f32> for F32Decoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> f32 {
        let mantissa_ptr = unsafe { self.mantissa.next_unchecked_as_ptr() };

        // Loading 4 bytes instead of 3 is 30% faster, so we read 1 extra byte after mantissa_ptr.
        // Safety: The extra byte is within bounds because sign_exp comes after mantissa.
        let mantissa_extended = unsafe { *(mantissa_ptr as *const [u8; 4]) };
        let mantissa = u32::from_le_bytes(mantissa_extended) & 0xFF_FF_FF;

        let sign_exp = unsafe { self.sign_exp.next_unchecked() };
        f32::from_bits(mantissa | ((sign_exp as u32) << 24))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn test() {
        for i in 1..16 {
            let mut rng = ChaCha20Rng::from_seed(Default::default());
            let floats: Vec<_> = (0..i).map(|_| f32::from_bits(rng.gen())).collect();

            let mut encoder = F32Encoder::default();
            encoder.reserve(NonZeroUsize::new(floats.len()).unwrap());
            for &f in &floats {
                encoder.encode(&f);
            }
            let bytes = encoder.collect();

            let mut decoder = F32Decoder::default();
            let mut slice = bytes.as_slice();
            decoder.populate(&mut slice, floats.len()).unwrap();
            assert!(slice.is_empty());
            for &f in &floats {
                assert_eq!(f.to_bits(), decoder.decode().to_bits());
            }
        }
    }

    fn bench_data() -> Vec<f32> {
        let mut rng = ChaCha20Rng::from_seed(Default::default());
        (0..crate::limit_bench_miri(1500001))
            .map(|_| rng.gen())
            .collect()
    }
    crate::bench_encode_decode!(f32_vec: Vec<f32>);
}
