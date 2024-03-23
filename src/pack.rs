use crate::coder::Result;
use crate::consume::{consume_byte, consume_byte_arrays, consume_bytes};
use crate::error::err;
use crate::fast::CowSlice;
use crate::pack_ints::SizedInt;

/// Possible states per byte in descending order. Each packed byte will use `log2(states)` bits.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, PartialOrd)]
enum Packing {
    _256 = 0,
    _16,
    _6,
    _4,
    _3,
    _2,
}

impl Packing {
    fn new(max: u8) -> Self {
        match max {
            // We could encode max 0 as nothing, but that could allocate unbounded memory when decoding.
            0..=1 => Self::_2,
            2 => Self::_3,
            3 => Self::_4,
            4..=5 => Self::_6,
            6..=15 => Self::_16,
            _ => Self::_256,
        }
    }

    fn write(self, out: &mut Vec<u8>, offset_by_min: bool) {
        // Encoded in such a way such that 0 is `Self::_256` and higher numbers are smaller packing.
        // Also makes `Self::_256` with offset_by_min = true is unrepresentable.
        out.push(self as u8 * 2 - offset_by_min as u8);
    }

    fn read(input: &mut &[u8]) -> Result<(Self, bool)> {
        let v = consume_byte(input)?;
        let p_u8 = crate::nightly::div_ceil_u8(v, 2);
        let offset_by_min = v & 1 != 0;
        let p = match p_u8 {
            0 => Self::_256,
            1 => Self::_16,
            2 => Self::_6,
            3 => Self::_4,
            4 => Self::_3,
            5 => Self::_2,
            _ => return invalid_packing(),
        };
        debug_assert_eq!(p as u8, p_u8);
        Ok((p, offset_by_min))
    }
}

pub(crate) fn invalid_packing<T>() -> Result<T> {
    err("invalid packing")
}

/// Packs 8 bools per byte.
pub fn pack_bools(bools: &[bool], out: &mut Vec<u8>) {
    pack_arithmetic::<2>(bytemuck::cast_slice(bools), out);
}

/// Unpacks 8 bools per byte. `out` will be overwritten with the bools.
pub fn unpack_bools(input: &mut &[u8], length: usize, out: &mut CowSlice<bool>) -> Result<()> {
    // TODO could borrow if length == 1.
    let mut set_owned = out.set_owned();
    let out: &mut Vec<bool> = &mut set_owned;
    // Safety: u8 and bool have same size/align and `out` will only contain bytes that are 0 or 1.
    let out: &mut Vec<u8> = unsafe { std::mem::transmute(out) };
    unpack_arithmetic::<2>(input, length, out)
}

fn skip_packing(length: usize) -> bool {
    length <= 2 // Packing takes at least 2 bytes, so it can only expand <= 2 bytes.
}

pub trait Byte: SizedInt {}
impl Byte for u8 {}
impl Byte for i8 {}

/// Packs multiple bytes into single bytes and writes them to `out`. This only works if
/// `max - min < 16`, otherwise this just copies `bytes` to `out`.
///
/// These particular tradeoffs were selected so input bytes don't span multiple output bytes to
/// avoid confusing bytewise compression algorithms (e.g. Deflate).
///
/// Mutates `bytes` to avoid copying them. The remaining `bytes` should be considered garbage.
pub fn pack_bytes<T: Byte>(bytes: &mut [T], out: &mut Vec<u8>) {
    if skip_packing(bytes.len()) {
        out.extend_from_slice(bytemuck::must_cast_slice(bytes));
        return;
    }
    let (min, max) = crate::pack_ints::minmax(bytes);

    // i8 packs as u8 if positive.
    let basic_packing = if min >= T::default() {
        Packing::new(bytemuck::must_cast(max))
    } else {
        Packing::_256 // Any negative i8 as u8 is > 15 and can't be packed without offset_packing.
    };

    // u8::wrapping_sub == i8::wrapping_sub, so we can use u8s from here onward.
    let min: u8 = bytemuck::must_cast(min);
    let max: u8 = bytemuck::must_cast(max);
    let bytes: &mut [u8] = bytemuck::must_cast_slice_mut(bytes);
    pack_bytes_unsigned(bytes, out, basic_packing, min, max);
}

/// [`pack_bytes`] but after i8s have been cast to u8s.
fn pack_bytes_unsigned(
    bytes: &mut [u8],
    out: &mut Vec<u8>,
    basic_packing: Packing,
    min: u8,
    max: u8,
) {
    // If subtracting min from all bytes results in a better packing do it, otherwise don't bother.
    let offset_packing = Packing::new(max.wrapping_sub(min));
    let p = if offset_packing > basic_packing && bytes.len() > 5 {
        for b in bytes.iter_mut() {
            *b = b.wrapping_sub(min);
        }
        offset_packing.write(out, true);
        out.push(min);
        offset_packing
    } else {
        basic_packing.write(out, false);
        basic_packing
    };

    match p {
        Packing::_256 => out.extend_from_slice(bytes),
        Packing::_16 => pack_arithmetic::<16>(bytes, out),
        Packing::_6 => pack_arithmetic::<6>(bytes, out),
        Packing::_4 => pack_arithmetic::<4>(bytes, out),
        Packing::_3 => pack_arithmetic::<3>(bytes, out),
        Packing::_2 => pack_arithmetic::<2>(bytes, out),
    }
}

/// Opposite of `pack_bytes`. Needs to know the `length` in bytes. `out` is overwritten with the bytes.
pub fn unpack_bytes<'a, T: Byte>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, T>,
) -> Result<()> {
    unpack_bytes_unsigned(input, length, out.cast_mut())
}

/// [`unpack_bytes`] but after i8s have been cast to u8s.
fn unpack_bytes_unsigned<'a>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, u8>,
) -> Result<()> {
    if skip_packing(length) {
        out.set_borrowed(consume_bytes(input, length)?);
        return Ok(());
    }

    let (p, offset_by_min) = Packing::read(input)?;
    let min = offset_by_min.then(|| consume_byte(input)).transpose()?;

    if p == Packing::_256 {
        debug_assert!(min.is_none()); // Packing::_256 with min should be unrepresentable.
        out.set_borrowed(consume_bytes(input, length)?);
        return Ok(());
    }

    let mut set_owned = out.set_owned();
    let out = &mut *set_owned;
    match p {
        Packing::_16 => unpack_arithmetic::<16>(input, length, out)?,
        Packing::_6 => unpack_arithmetic::<6>(input, length, out)?,
        Packing::_4 => unpack_arithmetic::<4>(input, length, out)?,
        Packing::_3 => unpack_arithmetic::<3>(input, length, out)?,
        Packing::_2 => unpack_arithmetic::<2>(input, length, out)?,
        Packing::_256 => unreachable!(),
    }
    if let Some(min) = min {
        for v in out {
            *v = v.wrapping_add(min);
        }
    }
    Ok(())
}

/// Like `pack_bytes` but all values are less than `N` so it can avoid encoding the packing.
pub fn pack_bytes_less_than<const N: usize>(bytes: &[u8], out: &mut Vec<u8>) {
    debug_assert!(bytes.iter().all(|&b| (b as usize) < N));
    match Packing::new(N.saturating_sub(1) as u8) {
        Packing::_256 => out.extend_from_slice(bytes),
        Packing::_16 => pack_arithmetic::<16>(bytes, out),
        Packing::_6 => pack_arithmetic::<6>(bytes, out),
        Packing::_4 => pack_arithmetic::<4>(bytes, out),
        Packing::_3 => pack_arithmetic::<3>(bytes, out),
        Packing::_2 => pack_arithmetic::<2>(bytes, out),
    }
}

/// Like `unpack_bytes` but all values are less than `N` so it can avoid encoding the packing.
/// Bytes returned by this function are guaranteed less than `N`.
///
/// If `HISTOGRAM` is set to `N` it also returns a histogram of the output bytes. This is because
/// the histogram can be calculated much faster when operating on the packed bytes.
///
/// If `HISTOGRAM` is set to `0` it only checks variants < `N` and doesn't calculate a histogram.
pub fn unpack_bytes_less_than<'a, const N: usize, const HISTOGRAM: usize>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, u8>,
) -> Result<[usize; HISTOGRAM]> {
    assert!(HISTOGRAM == N || HISTOGRAM == 0);

    /// Checks that `unpacked` bytes are less than `N`. All of `unpacked` is assumed to be < `FACTOR`.
    /// `HISTOGRAM` must be 0.
    fn check_less_than<const N: usize, const HISTOGRAM: usize, const FACTOR: usize>(
        unpacked: &[u8],
    ) -> Result<[usize; HISTOGRAM]> {
        assert!(FACTOR >= N);
        debug_assert!(unpacked.iter().all(|&v| (v as usize) < FACTOR));
        if FACTOR > N && unpacked.iter().copied().max().unwrap_or(0) as usize >= N {
            return invalid_packing();
        }
        Ok(std::array::from_fn(|_| unreachable!("HISTOGRAM not 0")))
    }

    /// Returns `Ok(histogram)` if buckets after `OUT` are 0.
    fn check_histogram<const IN: usize, const OUT: usize>(
        histogram: [usize; IN],
    ) -> Result<[usize; OUT]> {
        let (histogram, remaining) = histogram.split_at(OUT);
        if remaining.iter().copied().sum::<usize>() != 0 {
            return invalid_packing();
        }
        Ok(*<&[usize; OUT]>::try_from(histogram).unwrap())
    }

    let p = Packing::new(N.saturating_sub(1) as u8);
    if p == Packing::_256 {
        let bytes = consume_bytes(input, length)?;
        out.set_borrowed(bytes);
        return if HISTOGRAM == 0 {
            check_less_than::<N, HISTOGRAM, 256>(bytes)
        } else {
            check_histogram(crate::histogram::histogram(bytes))
        };
    }

    /// `FACTOR_POW_DIVISOR == (FACTOR as usize).pow(factor_to_divisor::<FACTOR>() as u32)` but as a constant.
    fn unpack_arithmetic_less_than<
        const N: usize,
        const HISTOGRAM: usize,
        const FACTOR: usize,
        const FACTOR_POW_DIVISOR: usize,
    >(
        input: &mut &[u8],
        length: usize,
        out: &mut Vec<u8>,
    ) -> Result<[usize; HISTOGRAM]> {
        assert!(HISTOGRAM == N || HISTOGRAM == 0);
        assert!(FACTOR >= 2 && FACTOR >= N);
        let divisor = factor_to_divisor::<FACTOR>();
        assert_eq!(FACTOR.pow(divisor as u32), FACTOR_POW_DIVISOR);

        let original_input = *input;
        unpack_arithmetic::<FACTOR>(input, length, out)?;
        if HISTOGRAM == 0 {
            check_less_than::<N, HISTOGRAM, FACTOR>(out)
        } else {
            let floor = length / divisor;
            let ceil = crate::nightly::div_ceil_usize(length, divisor);
            let whole = &original_input[..floor];

            // Can only `partial_with_garbage % FACTOR` partial_length times as the rest are undefined garbage.
            let partial_length = length - floor * divisor;
            let partial_with_garbage = original_input[floor..ceil].first().copied();

            // POPCNT is much faster than histogram.
            let histogram = if FACTOR == 2 {
                assert_eq!(N, 2);
                assert_eq!(divisor, 8);
                let mut one_count = 0;
                let mut whole = whole;
                while let Ok(chunk) = consume_byte_arrays(&mut whole, 1) {
                    one_count += u64::from_ne_bytes(chunk[0]).count_ones() as usize;
                }
                for &byte in whole {
                    one_count += byte.count_ones() as usize;
                }
                if let Some(partial_with_garbage) = partial_with_garbage {
                    // Set undefined garbage bits to zero.
                    let partial = partial_with_garbage << (divisor - partial_length);
                    one_count += partial.count_ones() as usize;
                }
                Ok(std::array::from_fn(|i| match i {
                    0 => length - one_count,
                    1 => one_count,
                    _ => unreachable!(),
                }))
            } else {
                check_histogram(if whole.len() < 100 {
                    // Simple path: histogram of unpacked bytes.
                    let mut histogram = [0; FACTOR];
                    for &v in out.iter() {
                        // Safety: unpack_arithmetic::<FACTOR> returns bytes < FACTOR.
                        unsafe { *histogram.get_unchecked_mut(v as usize) += 1 };
                    }
                    histogram
                } else {
                    // High throughput path: histogram of packed bytes (one time cost of ~100ns).
                    let packed_histogram = check_histogram::<256, FACTOR_POW_DIVISOR>(
                        crate::histogram::histogram(whole),
                    )?;
                    let mut histogram: [_; FACTOR] = unpack_histogram(&packed_histogram);
                    if let Some(mut partial_with_garbage) = partial_with_garbage {
                        // .min(divisor) does nothing, it's only improve code gen.
                        for _ in 0..partial_length.min(divisor) {
                            histogram[partial_with_garbage as usize % FACTOR] += 1;
                            partial_with_garbage /= FACTOR as u8;
                        }
                    }
                    histogram
                })
            };
            if let Ok(h) = histogram {
                debug_assert_eq!(
                    h,
                    check_histogram(crate::histogram::histogram(out)).unwrap()
                );
            }
            histogram
        }
    }

    let mut set_owned = out.set_owned();
    let out = &mut *set_owned;
    match p {
        Packing::_16 => unpack_arithmetic_less_than::<N, HISTOGRAM, 16, 256>(input, length, out),
        Packing::_6 => unpack_arithmetic_less_than::<N, HISTOGRAM, 6, 216>(input, length, out),
        Packing::_4 => unpack_arithmetic_less_than::<N, HISTOGRAM, 4, 256>(input, length, out),
        Packing::_3 => unpack_arithmetic_less_than::<N, HISTOGRAM, 3, 243>(input, length, out),
        Packing::_2 => unpack_arithmetic_less_than::<N, HISTOGRAM, 2, 256>(input, length, out),
        Packing::_256 => unreachable!(),
    }
}

#[inline(never)]
fn unpack_histogram<const FACTOR: usize, const FACTOR_POW_DIVISOR: usize>(
    packed_histogram: &[usize; FACTOR_POW_DIVISOR],
) -> [usize; FACTOR] {
    let divisor = factor_to_divisor::<FACTOR>();
    assert_eq!(FACTOR.pow(divisor as u32), FACTOR_POW_DIVISOR);
    std::array::from_fn(|i| {
        let mut sum = 0;
        for level in 0..divisor {
            let width = FACTOR.pow(level as u32);
            let runs = FACTOR_POW_DIVISOR / (width * FACTOR);
            for run in 0..runs {
                let run_start = run * (width * FACTOR) + i * width;
                let section = &packed_histogram[run_start..run_start + width];
                sum += section.iter().copied().sum::<usize>();
            }
        }
        sum
    })
}

#[inline(always)]
fn factor_to_divisor<const FACTOR: usize>() -> usize {
    match FACTOR {
        2 => 8,
        3 => 5,
        4 => 4,
        6 => 3,
        16 => 2,
        _ => unreachable!(),
    }
}

const BMI2: bool = cfg!(all(
    target_arch = "x86_64",
    target_feature = "bmi2",
    not(miri)
));

/// Packs multiple bytes into one. All the bytes must be < `FACTOR`.
/// Factors 2,4,16 are bit packing. Factors 3,6 are arithmetic coding.
fn pack_arithmetic<const FACTOR: usize>(bytes: &[u8], out: &mut Vec<u8>) {
    debug_assert!(bytes.iter().all(|&v| v < FACTOR as u8));
    let divisor = factor_to_divisor::<FACTOR>();

    let floor = bytes.len() / divisor;
    let ceil = (bytes.len() + (divisor - 1)) / divisor;

    out.reserve(ceil);
    let packed = &mut out.spare_capacity_mut()[..ceil];

    for i in 0..floor {
        unsafe {
            packed.get_unchecked_mut(i).write(if FACTOR == 2 && BMI2 {
                #[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
                unreachable!();
                #[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
                {
                    // Could use on any pow2 FACTOR, but only 2 is faster (target-cpu=native).
                    let chunk = (bytes.as_ptr() as *const u8 as *const [u8; 8]).add(i);
                    let chunk = u64::from_le_bytes(*chunk);
                    std::arch::x86_64::_pext_u64(chunk, 0x0101010101010101) as u8
                }
            } else {
                let mut acc = 0;
                for byte_index in 0..divisor {
                    let byte = *bytes.get_unchecked(i * divisor + byte_index);
                    acc += byte * (FACTOR as u8).pow(byte_index as u32);
                }
                acc
            });
        }
    }
    if floor < ceil {
        let mut acc = 0;
        for &v in bytes[floor * divisor..].iter().rev() {
            acc *= FACTOR as u8;
            acc += v;
        }
        packed[floor].write(acc);
    }
    // Safety: `ceil` elements after len were initialized by loops above.
    unsafe { out.set_len(out.len() + ceil) };
}

/// Opposite of `pack_arithmetic`. `out` will be overwritten with the unpacked bytes.
fn unpack_arithmetic<const FACTOR: usize>(
    input: &mut &[u8],
    unpacked_len: usize,
    out: &mut Vec<u8>,
) -> Result<()> {
    let divisor = factor_to_divisor::<FACTOR>();

    // TODO STRICT: check that packed.all(|&b| b < FACTOR.powi(divisor)).
    let floor = unpacked_len / divisor;
    let ceil = crate::nightly::div_ceil_usize(unpacked_len, divisor);
    let packed = consume_bytes(input, ceil)?;

    debug_assert!(out.is_empty());
    out.reserve(unpacked_len);
    let unpacked = &mut out.spare_capacity_mut()[..unpacked_len];

    for i in 0..floor {
        unsafe {
            let mut packed = *packed.get_unchecked(i);
            if FACTOR == 2 && BMI2 {
                #[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
                unreachable!();
                #[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
                {
                    // Could use on any pow2 FACTOR, but only 2 is faster (target-cpu=native).
                    let chunk = std::arch::x86_64::_pdep_u64(packed as u64, 0x0101010101010101);
                    *(unpacked.as_mut_ptr() as *mut [u8; 8]).add(i) = chunk.to_le_bytes();
                }
            } else {
                for byte in unpacked.get_unchecked_mut(i * divisor..i * divisor + divisor) {
                    byte.write(packed % FACTOR as u8);
                    packed /= FACTOR as u8;
                }
            }
        }
    }
    if floor < ceil {
        let mut packed = packed[floor];
        for byte in unpacked[floor * divisor..].iter_mut() {
            byte.write(packed % FACTOR as u8);
            packed /= FACTOR as u8;
        }
    }
    // Safety: `unpacked_len` elements were initialized by the loops above.
    unsafe { out.set_len(unpacked_len) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::error::err;
    use paste::paste;
    use test::{black_box, Bencher};

    fn pack_bytes<T: super::Byte>(bytes: &[T]) -> Vec<u8> {
        let mut out = vec![];
        super::pack_bytes(&mut bytes.to_owned(), &mut out);
        out
    }

    fn unpack_bytes<T: super::Byte>(mut packed: &[u8], length: usize) -> Vec<T> {
        let mut out = crate::fast::CowSlice::default();
        super::unpack_bytes(&mut packed, length, &mut out).unwrap();
        assert!(packed.is_empty());
        unsafe { out.as_slice(length).to_vec() }
    }

    #[test]
    fn test_pack_bytes_u8() {
        assert_eq!(pack_bytes(&[1u8, 2, 3, 4, 5, 6, 7]).len(), 5);
        assert_eq!(pack_bytes(&[201u8, 202, 203, 204, 205, 206, 207]).len(), 6);

        for max in 0..255u8 {
            for sub in [1, 2, 3, 4, 5, 15, 255] {
                let min = max.saturating_sub(sub);
                let original = [min, min, min, min, min, min, min, max];
                let packed = pack_bytes(&original);
                let unpacked = unpack_bytes(&packed, original.len());
                assert_eq!(original.as_slice(), unpacked.as_slice());
            }
        }
    }

    #[test]
    fn test_pack_bytes_i8() {
        assert_eq!(pack_bytes(&[1i8, 2, 3, 4, 5, 6, 7]).len(), 5);
        assert_eq!(pack_bytes(&[-1i8, -2, -3, -4, -5, -6, -7]).len(), 6);
        assert_eq!(pack_bytes(&[-3i8, -2, -1, 0, 1, 2, 3]).len(), 6);
        assert_eq!(
            pack_bytes(&[0i8, -1, 0, -1, 0, -1, 0]),
            [9, (-1i8) as u8, 0b1010101]
        );

        for max in i8::MIN..i8::MAX {
            for sub in [1, 2, 3, 4, 5, 15, 127] {
                let min = max.saturating_sub(sub);
                let original = [min, min, min, min, min, min, min, max];
                let packed = pack_bytes(&original);
                let unpacked = unpack_bytes(&packed, original.len());
                assert_eq!(original.as_slice(), unpacked.as_slice());
            }
        }
    }

    #[test]
    fn unpack_bytes_errors() {
        assert_eq!(
            super::unpack_bytes::<u8>(&mut [1].as_slice(), 5, &mut Default::default()),
            err("EOF")
        );
        assert_eq!(
            super::unpack_bytes::<u8>(&mut [255].as_slice(), 5, &mut Default::default()),
            super::invalid_packing()
        );
    }

    fn pack_arithmetic<const FACTOR: usize>(bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![];
        super::pack_arithmetic::<FACTOR>(bytes, &mut out);
        out
    }

    #[test]
    fn test_pack_arithmetic() {
        assert_eq!(pack_arithmetic::<2>(&[1, 0, 1, 0]), [0b0101]);
        assert_eq!(
            pack_arithmetic::<2>(&[1, 0, 1, 0, 1, 0, 1, 0]),
            [0b01010101]
        );
        assert_eq!(
            pack_arithmetic::<2>(&[1, 0, 1, 0, 1, 0, 1, 0, 1]),
            [0b01010101, 0b1]
        );

        assert_eq!(pack_arithmetic::<3>(&[0]), [0]);
        assert_eq!(pack_arithmetic::<3>(&[0, 1]), [0 + 1 * 3]);
        assert_eq!(pack_arithmetic::<3>(&[0, 1, 2]), [0 + 1 * 3 + 2 * 3 * 3]);
        assert_eq!(
            pack_arithmetic::<3>(&[2, 0, 0, 0, 0, 0, 1, 2]),
            [2, 0 + 1 * 3 + 2 * 3 * 3]
        );

        assert_eq!(pack_arithmetic::<4>(&[1, 0]), [0b0001]);
        assert_eq!(pack_arithmetic::<4>(&[1, 0, 1, 0]), [0b00010001]);
        assert_eq!(
            pack_arithmetic::<4>(&[1, 0, 1, 0, 1, 0]),
            [0b00010001, 0b0001]
        );

        assert_eq!(pack_arithmetic::<6>(&[0]), [0]);
        assert_eq!(pack_arithmetic::<6>(&[0, 1]), [0 + 1 * 6]);
        assert_eq!(pack_arithmetic::<6>(&[0, 1, 2]), [0 + 1 * 6 + 2 * 6 * 6]);
        assert_eq!(
            pack_arithmetic::<6>(&[2, 0, 0, 0, 1, 2]),
            [2, 0 + 1 * 6 + 2 * 6 * 6]
        );

        assert_eq!(pack_arithmetic::<16>(&[1]), [0b0001]);
        assert_eq!(pack_arithmetic::<16>(&[1, 0]), [0b00000001]);
        assert_eq!(pack_arithmetic::<16>(&[1, 0, 1]), [0b00000001, 0b0001]);
    }

    #[test]
    fn test_unpack_arithmetic() {
        fn test<const FACTOR: usize>(bytes: &[u8]) {
            let packed = pack_arithmetic::<FACTOR>(bytes);

            let mut input = packed.as_slice();
            let mut bytes2 = vec![];
            super::unpack_arithmetic::<FACTOR>(&mut input, bytes.len(), &mut bytes2).unwrap();
            assert!(input.is_empty());
            assert_eq!(bytes, bytes2);
        }

        test::<2>(&[1, 0, 1, 0]);
        test::<2>(&[1, 0, 1, 0, 1, 0, 1, 0]);
        test::<2>(&[1, 0, 1, 0, 1, 0, 1, 0, 1]);

        test::<3>(&[0]);
        test::<3>(&[0, 1]);
        test::<3>(&[0, 1, 2]);
        test::<3>(&[2, 0, 0, 0, 0, 0, 1, 2]);

        test::<4>(&[1, 0]);
        test::<4>(&[1, 0, 1, 0]);
        test::<4>(&[1, 0, 1, 0, 1, 0]);

        test::<6>(&[0]);
        test::<6>(&[0, 1]);
        test::<6>(&[0, 1, 2]);
        test::<6>(&[2, 0, 0, 0, 1, 2]);

        test::<16>(&[1]);
        test::<16>(&[1, 0]);
        test::<16>(&[1, 0, 1]);
    }

    fn bench_pack_arithmetic<const FACTOR: usize>(b: &mut Bencher) {
        let bytes = vec![0; 1000];
        let mut out = Vec::with_capacity(bytes.len());
        b.iter(|| {
            out.clear();
            super::pack_arithmetic::<FACTOR>(&bytes, black_box(&mut out));
        });
    }

    fn bench_unpack_arithmetic<const FACTOR: usize>(b: &mut Bencher) {
        let unpacked_len = 1000;
        let packed = pack_arithmetic::<FACTOR>(&vec![0; unpacked_len]);
        let mut out = Vec::with_capacity(unpacked_len);

        b.iter(|| {
            let mut input = packed.as_slice();
            let input = black_box(&mut input);
            let unpacked_len = black_box(unpacked_len);
            out.clear();
            super::unpack_arithmetic::<FACTOR>(input, unpacked_len, black_box(&mut out)).unwrap();
        });
    }

    macro_rules! bench_n {
        ($bench:ident, $($n:literal),+) => {
            paste! {
                $(
                    #[bench]
                    fn [<$bench $n>](b: &mut Bencher) {
                        $bench::<$n>(b);
                    }
                )+
            }
        }
    }
    bench_n!(bench_pack_arithmetic, 2, 3, 4, 6, 16);
    bench_n!(bench_unpack_arithmetic, 2, 3, 4, 6, 16);

    fn test_pack_bytes_less_than_n<const N: usize, const FACTOR: usize>() {
        for n in [1, 11, 97, 991, 10007].into_iter().flat_map(|n_prime| {
            let divisor = if FACTOR == 256 {
                1
            } else {
                super::factor_to_divisor::<FACTOR>()
            };
            let n_factor = crate::nightly::div_ceil_usize(n_prime, divisor) * divisor;
            [n_factor, n_prime]
        }) {
            let bytes: Vec<_> = crate::random_data(n)
                .into_iter()
                .map(|v: usize| (v % N as usize) as u8)
                .collect();
            let n = bytes.len(); // random_data shrinks n on miri.

            println!("n {n}, N {N}, FACTOR {FACTOR}");
            if N != FACTOR {
                let mut bytes = bytes.clone();
                bytes[n - 1] = (FACTOR - 1) as u8; // Make least 1 byte is out of bounds.
                let mut packed = vec![];
                super::pack_bytes_less_than::<FACTOR>(&bytes, &mut packed);

                assert!(super::unpack_bytes_less_than::<N, 0>(
                    &mut packed.as_slice(),
                    bytes.len(),
                    &mut crate::fast::CowSlice::default()
                )
                .is_err());
                assert!(super::unpack_bytes_less_than::<N, N>(
                    &mut packed.as_slice(),
                    bytes.len(),
                    &mut crate::fast::CowSlice::default()
                )
                .is_err());
            }

            let mut packed = vec![];
            super::pack_bytes_less_than::<N>(&bytes, &mut packed);

            let mut input = packed.as_slice();
            let mut unpacked = crate::fast::CowSlice::default();
            super::unpack_bytes_less_than::<N, 0>(&mut input, bytes.len(), &mut unpacked).unwrap();
            assert!(input.is_empty());
            assert_eq!(unsafe { unpacked.as_slice(bytes.len()) }, bytes);

            let mut input = packed.as_slice();
            let mut unpacked = crate::fast::CowSlice::default();
            let histogram =
                super::unpack_bytes_less_than::<N, N>(&mut input, bytes.len(), &mut unpacked)
                    .unwrap();
            assert!(input.is_empty());
            assert_eq!(unsafe { unpacked.as_slice(bytes.len()) }, bytes);
            assert_eq!(
                histogram.as_slice(),
                &crate::histogram::histogram(&bytes)[..N]
            );
        }
    }

    macro_rules! test_pack_bytes_less_than_n {
        ($($n:literal => $factor:literal),+) => {
            $(
                paste::paste! {
                    #[test]
                    fn [<test_pack_bytes_less_than_ $n>]() {
                        test_pack_bytes_less_than_n::<$n, $factor>();
                    }
                }
            )+
        }
    }
    // Test factors and +/- 1 to catch off by 1 errors.
    test_pack_bytes_less_than_n!(2 => 2, 3 => 3, 4 => 4, 5 => 6, 6 => 6, 7 => 16);
    test_pack_bytes_less_than_n!(15 => 16, 16 => 16, 17 => 256, 255 => 256, 256 => 256);

    macro_rules! bench_unpack_histogram {
        ($($f:literal => $fpd:literal),+) => {
            $(
                paste::paste! {
                    #[bench]
                    fn [<bench_unpack_histogram $f>](b: &mut Bencher) {
                        b.iter(|| {
                            super::unpack_histogram::<$f, $fpd>(black_box(&[0; $fpd]))
                        });
                    }
                }
            )+
        }
    }
    bench_unpack_histogram!(3 => 243, 4 => 256, 6 => 216, 16 => 256);

    macro_rules! bench_unpack_bytes_less_than {
        ($($n:literal),+) => {
            $(
                paste::paste! {
                    #[bench]
                    fn [<bench_unpack_bytes_less_than $n>](b: &mut Bencher) {
                        let mut out = crate::fast::CowSlice::default();
                        b.iter(|| {
                            super::unpack_bytes_less_than::<$n, 0>(black_box(&mut [0].as_slice()), black_box(1), black_box(&mut out)).unwrap();
                        });
                    }

                    #[bench]
                    fn [<bench_unpack_bytes_less_than $n _histogram>](b: &mut Bencher) {
                        let mut out = crate::fast::CowSlice::default();
                        b.iter(|| {
                            super::unpack_bytes_less_than::<$n, $n>(black_box(&mut [0].as_slice()), black_box(1), black_box(&mut out)).unwrap();
                        });
                    }
                }
            )+
        }
    }
    bench_unpack_bytes_less_than!(2, 3, 4, 6, 16, 256);
}
