use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::error::{err, error};
use crate::fast::{CowSlice, NextUnchecked, VecImpl};
use crate::int::{IntDecoder, IntEncoder};
use crate::pack::{pack_bytes, unpack_bytes};
use alloc::vec::Vec;
use core::num::NonZeroUsize;

#[derive(Default)]
pub struct LengthEncoder {
    small: VecImpl<u8>,
    large: IntEncoder<usize>,
}

impl Encoder<usize> for LengthEncoder {
    #[inline(always)]
    fn encode(&mut self, &v: &usize) {
        unsafe {
            let end_ptr = self.small.end_ptr();
            if v < 255 {
                *end_ptr = v as u8;
            } else {
                #[cold]
                #[inline(never)]
                unsafe fn encode_slow(end_ptr: *mut u8, large: &mut IntEncoder<usize>, v: usize) {
                    *end_ptr = 255;
                    large.reserve(NonZeroUsize::new(1).unwrap());
                    large.encode(&v);
                }
                encode_slow(end_ptr, &mut self.large, v);
            }
            self.small.increment_len();
        }
    }
}

pub trait Len {
    fn len(&self) -> usize;
}

impl<T> Len for &[T] {
    #[inline(always)]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}

impl LengthEncoder {
    /// Encodes a length known to be < `255`.
    #[cfg(feature = "arrayvec")]
    #[inline(always)]
    pub fn encode_less_than_255(&mut self, n: usize) {
        use crate::fast::PushUnchecked;
        debug_assert!(n < 255);
        unsafe { self.small.push_unchecked(n as u8) };
    }

    /// Encodes lengths less than `N`. Have to reserve `N * i.size_hint().1 elements`.
    /// Skips calling encode for T::len() == 0. Returns `true` if it failed due to a length over `N`.
    #[inline(always)]
    pub fn encode_vectored_max_len<T: Len, const N: usize>(
        &mut self,
        i: impl Iterator<Item = T>,
        mut encode: impl FnMut(T),
    ) -> bool {
        debug_assert!(N <= 64);
        let mut ptr = self.small.end_ptr();
        for t in i {
            let n = t.len();
            unsafe {
                *ptr = n as u8;
                ptr = ptr.add(1);
            }
            if n == 0 {
                continue;
            }
            if n > N {
                // Don't set end ptr (elements won't be saved).
                return true;
            }
            encode(t);
        }
        self.small.set_end_ptr(ptr);
        false
    }

    #[inline(always)]
    pub fn encode_vectored_fallback<T: Len>(
        &mut self,
        i: impl Iterator<Item = T>,
        mut reserve_and_encode_large: impl FnMut(T),
    ) {
        for v in i {
            let n = v.len();
            self.encode(&n);
            reserve_and_encode_large(v);
        }
    }
}

impl Buffer for LengthEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        pack_bytes(self.small.as_mut_slice(), out);
        self.small.clear();
        self.large.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.small.reserve(additional.get()); // All lengths inhabit small, only large ones inhabit large.
    }
}

#[derive(Default)]
pub struct LengthDecoder<'a> {
    small: CowSlice<'a, u8>,
    large: IntDecoder<'a, usize>,
    sum: usize,
}

impl<'a> LengthDecoder<'a> {
    pub fn length(&self) -> usize {
        self.sum
    }

    // For decoding lengths multiple times (e.g. ArrayVec, utf8 validation).
    pub fn borrowed_clone<'me: 'a>(&'me self) -> LengthDecoder<'me> {
        let mut small = CowSlice::default();
        small.set_borrowed_slice_impl(self.small.ref_slice().clone());
        Self {
            small,
            large: self.large.borrowed_clone(),
            sum: self.sum,
        }
    }

    /// Returns if any of the decoded lengths are > `N`.
    /// Safety: `length` must be the `length` passed to populate.
    #[cfg_attr(not(feature = "arrayvec"), allow(unused))]
    pub unsafe fn any_greater_than<const N: usize>(&self, length: usize) -> bool {
        if N < 255 {
            // Fast path: don't need to scan large lengths since there shouldn't be any.
            // A large length will have a 255 in small which will be greater than N.
            self.small
                .as_slice(length)
                .iter()
                .copied()
                .max()
                .unwrap_or(0) as usize
                > N
        } else {
            let mut decoder = self.borrowed_clone();
            (0..length).any(|_| decoder.decode() > N)
        }
    }
}

impl<'a> View<'a> for LengthDecoder<'a> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        unpack_bytes(input, length, &mut self.small)?;
        let small = unsafe { self.small.as_slice(length) };

        // Summing &[u8] can't overflow since that would require > 2^56 bytes of memory.
        let mut sum: u64 = small.iter().map(|&v| v as u64).sum();

        // Fast path for small lengths: If sum(small) < 255 every small < 255 so large_length is 0.
        if sum < 255 {
            self.sum = sum as usize;
            return Ok(());
        }

        // Every 255 byte indicates a large is present.
        let large_length = small.iter().filter(|&&v| v == 255).count();
        self.large.populate(input, large_length)?;

        // Can't overflow since sum includes large_length many 255s.
        sum -= large_length as u64 * 255;

        // Summing &[u64] can overflow, so we check it.
        let mut decoder = self.large.borrowed_clone();
        for _ in 0..large_length {
            let v: usize = decoder.decode();
            sum = sum
                .checked_add(v as u64)
                .ok_or_else(|| error("length overflow"))?;
        }
        if sum >= HUGE_LEN {
            return err("huge length"); // Lets us optimize decode with unreachable_unchecked.
        }
        self.sum = sum.try_into().map_err(|_| error("length > usize::MAX"))?;
        Ok(())
    }
}

// isize::MAX / (largest type we want to allocate without possibility of overflow)
const HUGE_LEN: u64 = 0x7FFFFFFF_FFFFFFFF / 4096;

impl<'a> Decoder<'a, usize> for LengthDecoder<'a> {
    #[inline(always)]
    fn decode(&mut self) -> usize {
        let length = unsafe {
            let v = self.small.mut_slice().next_unchecked();

            if v < 255 {
                v as usize
            } else {
                #[cold]
                unsafe fn cold(large: &mut IntDecoder<'_, usize>) -> usize {
                    large.decode()
                }
                cold(&mut self.large)
            }
        };

        // Allows some checks in Vec::with_capacity to be removed if lto = true.
        // Safety: sum < HUGE_LEN is checked in populate so all elements have to be < HUGE_LEN.
        if length as u64 >= HUGE_LEN {
            unsafe { core::hint::unreachable_unchecked() }
        }
        length
    }
}

#[cfg(test)]
mod tests {
    use super::{LengthDecoder, LengthEncoder};
    use crate::coder::{Buffer, Decoder, Encoder, View};
    use core::num::NonZeroUsize;

    #[test]
    fn test() {
        let mut encoder = LengthEncoder::default();
        encoder.reserve(NonZeroUsize::new(3).unwrap());
        encoder.encode(&1);
        encoder.encode(&255);
        encoder.encode(&2);
        let bytes = encoder.collect();

        let mut decoder = LengthDecoder::default();
        decoder.populate(&mut bytes.as_slice(), 3).unwrap();
        assert_eq!(decoder.decode(), 1);
        assert_eq!(decoder.decode(), 255);
        assert_eq!(decoder.decode(), 2);
    }

    #[cfg(target_pointer_width = "64")] // HUGE_LEN > u32::MAX
    #[test]
    fn huge_len() {
        for (x, is_ok) in [(super::HUGE_LEN - 1, true), (super::HUGE_LEN, false)] {
            let mut encoder = LengthEncoder::default();
            encoder.reserve(NonZeroUsize::new(1).unwrap());
            encoder.encode(&(x as usize));
            let bytes = encoder.collect();

            let mut decoder = LengthDecoder::default();
            assert_eq!(decoder.populate(&mut bytes.as_slice(), 1).is_ok(), is_ok);
        }
    }
}
