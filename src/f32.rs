use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::consume::consume_byte_arrays;
use crate::fast::{FastSlice, NextUnchecked, PushUnchecked, VecImpl};
use alloc::vec::Vec;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct F32Encoder(VecImpl<f32>);

impl Encoder<f32> for F32Encoder {
    #[inline(always)]
    fn as_primitive(&mut self) -> Option<&mut VecImpl<f32>> {
        Some(&mut self.0)
    }

    #[inline(always)]
    fn encode(&mut self, t: &f32) {
        unsafe { self.0.push_unchecked(*t) };
    }
}

pub const CHUNK_SIZE: usize = 16;

// CHUNK_SIZE = 16 with #[inline(never)] seems to be the sweet spot for both x86_64 and x86_64 target-cpu=native.
// Larger and it starts loading `chunk` multiple times. Smaller and it doesn't vectorize as well.
// Removing #[inline(never)] makes this autovectorization inconsistent.
// Safety: Same as `encode_tail`.
#[inline(never)]
unsafe fn encode_chunk(chunk: &[f32; CHUNK_SIZE], mantissa: *mut [u8; 3], sign_exp: *mut u8) {
    encode_tail(chunk, mantissa, sign_exp)
}

// Safety:
// `mantissa` must have `tail.len() * 3 + 1 (if tail not empty)` bytes valid for writes.
// `sign_exp` must have `tail.len()` bytes valid for writes (maybe aliasing with mantissa so ptrs are required).
unsafe fn encode_tail(tail: &[f32], mantissa: *mut [u8; 3], sign_exp: *mut u8) {
    for (i, &f) in tail.iter().enumerate() {
        let little_endian = f.to_le_bytes();
        // Writing overlapping 4 byte mantissas in a separate loops from sign_exp is 70% faster
        // than splitting chunks of 4 f32 with bitshifts and ~3.3x faster the scalar solution.
        // Safety: `mantissa` has `tail.len() * 3 + 1 (tail is not empty)` bytes valid for writes.
        *(mantissa.add(i) as *mut [u8; 4]) = little_endian;
    }

    for (i, &f) in tail.iter().enumerate() {
        // Safety: `sign_exp` has `tail.len()` bytes valid for writes (maybe aliasing with mantissa so ptrs are used).
        *sign_exp.add(i) = f.to_le_bytes()[3];
    }
}

impl Buffer for F32Encoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        let floats = self.0.as_slice();
        let Some(first_float) = floats.get(0).copied() else {
            return;
        };
        let byte_len = core::mem::size_of_val(floats);
        out.reserve(byte_len);
        let mantissa_start = out.spare_capacity_mut().as_mut_ptr() as *mut [u8; 3];

        // Safety: we've allocated floats.len() * 4 bytes past the end out of out.
        // Therefore, the pointer at byte floats.len() * 3 is not past the end of the allocation.
        let sign_exp_chunks = unsafe { mantissa_start.add(floats.len()) as *mut [u8; CHUNK_SIZE] };
        let mantissa_chunks = mantissa_start as *mut [[u8; 3]; CHUNK_SIZE];

        let (chunks, tail) = floats.as_chunks::<CHUNK_SIZE>();
        for (i, chunk) in chunks.iter().enumerate() {
            // Safety:
            // `mantissa`: We've allocated floats.len() * 4 bytes, so we have `floats.len() * 4` bytes are valid for writes.
            //             `floats.len() * 3 + 1 (if tail not empty)` is always <= floats.len() * 4.
            // `sign_exp`: We've allocated floats.len() * 4 bytes so the pointer starting at floats.len() * 3 has floats.len() valid bytes.
            //             We keep everying as raw pointers so the aliasing with mantissa's last byte is valid.
            unsafe {
                let mantissa = mantissa_chunks.add(i) as *mut [u8; 3];
                let sign_exp = sign_exp_chunks.add(i) as *mut u8;
                encode_chunk(chunk, mantissa, sign_exp);
            }
        }
        // Safety: same as above call to encode_chunk.
        unsafe {
            let mantissa = mantissa_chunks.add(chunks.len()) as *mut [u8; 3];
            let sign_exp = sign_exp_chunks.add(chunks.len()) as *mut u8;
            encode_tail(tail, mantissa, sign_exp);
        }

        // Fix up the sign_exp killed by the last 3 byte mantissa writing 4 bytes (technically only required if !chunks.is_empty()).
        // Safety: sign_exp_chunks is not past the end of the allocation.
        //         Additionally floats.len() * 3 < floats.len() * 4 because we've ensured
        //         floats isn't empty, so this 1 byte u8 pointer is inside the allocation.
        unsafe { *(sign_exp_chunks as *mut u8) = first_float.to_le_bytes()[3] };

        // Safety: We just initialized these elements in the loops above.
        unsafe { out.set_len(out.len() + byte_len) };
        self.0.clear();
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional.get());
    }
}

#[derive(Default)]
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
        // CHUNK_SIZE * 3 exhibits all sizes of the tail, the first chunk and the second chunk.
        for i in 0..CHUNK_SIZE * 3 {
            let mut rng = ChaCha20Rng::from_seed(Default::default());
            let floats: Vec<_> = (0..i).map(|_| f32::from_bits(rng.gen())).collect();

            let mut encoder = F32Encoder::default();
            if let Some(additional) = NonZeroUsize::new(floats.len()) {
                encoder.reserve(additional);
            }
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
        crate::random_data::<f32>(1500001)
    }
    crate::bench_encode_decode!(f32_vec: Vec<f32>);
}

#[cfg(test)]
mod tests2 {
    use alloc::vec::Vec;

    fn bench_data() -> Vec<Vec<f32>> {
        crate::random_data::<u8>(125)
            .into_iter()
            .map(|n| (0..n / 16).map(|_| 0.0).collect())
            .collect()
    }
    crate::bench_encode_decode!(f32_vecs: Vec<Vec<f32>>);
}
