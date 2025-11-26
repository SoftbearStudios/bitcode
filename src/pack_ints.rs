use crate::coder::Result;
use crate::consume::{consume_byte, consume_byte_arrays};
use crate::error::error;
use crate::fast::CowSlice;
use crate::pack::{invalid_packing, pack_bytes, unpack_bytes};
use crate::Error;
use alloc::vec::Vec;
use bytemuck::Pod;

/// Possible integer sizes in descending order.
/// TODO consider nonstandard sizes like 24.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, PartialOrd)]
enum Packing {
    _128 = 0,
    _64,
    _32,
    _16,
    _8,
}

impl Packing {
    fn new<T: SizedUInt>(max: T) -> Self {
        let max: u128 = max.try_into().unwrap_or_else(|_| unreachable!()); // From<usize> isn't implemented for u128.
        #[allow(clippy::match_overlapping_arm)] // Just make sure not to reorder them.
        match max {
            ..=0xFF => Self::_8,
            ..=0xFF_FF => Self::_16,
            ..=0xFF_FF_FF_FF => Self::_32,
            ..=0xFF_FF_FF_FF_FF_FF_FF_FF => Self::_64,
            _ => Self::_128,
        }
    }

    fn write<T: SizedUInt>(self, out: &mut Vec<u8>, offset_by_min: bool) {
        // Encoded in such a way such that 0 is no packing and higher numbers are smaller packing.
        // Also makes no packing with offset_by_min = true is unrepresentable.
        out.push((self as u8 - Self::new(T::MAX) as u8) * 2 - offset_by_min as u8);
    }

    fn read<T: SizedUInt>(input: &mut &[u8]) -> Result<(Self, bool)> {
        let v = consume_byte(input)?;
        let p_u8 = crate::nightly::div_ceil_u8(v, 2) + Self::new(T::MAX) as u8;
        let offset_by_min = v & 1 != 0;
        let p = match p_u8 {
            0 => Self::_128,
            1 => Self::_64,
            2 => Self::_32,
            3 => Self::_16,
            4 => Self::_8,
            _ => return invalid_packing(),
        };
        debug_assert_eq!(p as u8, p_u8);
        Ok((p, offset_by_min))
    }
}

fn usize_too_big() -> Error {
    error("encountered a isize/usize with more than 32 bits on a 32 bit platform")
}

pub trait Int: Copy + core::fmt::Debug + Default + Ord + Pod + Send + Sized + Sync {
    // Unaligned native endian. TODO could be aligned on big endian since we always have to copy.
    type Une: Pod + Default + Send + Sync;
    type Int: SizedInt;
    #[inline]
    fn from_unaligned(unaligned: Self::Une) -> Self {
        bytemuck::must_cast(unaligned)
    }
    #[inline]
    fn to_unaligned(self) -> Self::Une {
        bytemuck::must_cast(self)
    }
    fn with_input(ints: &mut [Self], f: impl FnOnce(&mut [Self::Int]));
    fn with_output<'a>(
        out: &mut CowSlice<'a, Self::Une>,
        length: usize,
        f: impl FnOnce(&mut CowSlice<'a, <Self::Int as Int>::Une>) -> Result<()>,
    ) -> Result<()>;
}
macro_rules! impl_usize_and_isize {
    ($($isize:ident => $i64:ident),+) => {
        $(
            impl Int for $isize {
                type Une = [u8; core::mem::size_of::<Self>()];
                type Int = $i64;
                fn with_input(ints: &mut [Self], f: impl FnOnce(&mut [Self::Int])) {
                    if cfg!(target_pointer_width = "64") {
                        f(bytemuck::cast_slice_mut(ints))
                    } else {
                        // 32 bit isize to i64 requires conversion. TODO reuse allocation.
                        let mut ints: Vec<$i64> = ints.iter().map(|&v| v as $i64).collect();
                        f(&mut ints);
                    }
                }
                fn with_output<'a>(out: &mut CowSlice<'a, Self::Une>, length: usize, f: impl FnOnce(&mut CowSlice<'a, <Self::Int as Int>::Une>) -> Result<()>) -> Result<()> {
                    if cfg!(target_pointer_width = "64") {
                        f(out.cast_mut())
                    } else {
                        // i64 to 32 bit isize on requires checked conversion. TODO reuse allocations.
                        let mut out_i64 = CowSlice::default();
                        f(&mut out_i64)?;
                        let out_i64 = unsafe { out_i64.as_slice(length) };
                        let out_isize: Result<Vec<Self::Une>> = out_i64.iter().map(|&v| $i64::from_unaligned(v).try_into().map(Self::to_unaligned).map_err(|_| usize_too_big())).collect();
                        *out.set_owned() = out_isize?;
                        Ok(())
                    }
                }
            }
        )+
    }
}
impl_usize_and_isize!(usize => u64, isize => i64);

/// An [`Int`] that has a fixed size independent of platform (not usize).
pub trait SizedInt: Int {
    type Unsigned: SizedUInt;
    const MIN: Self;
    const MAX: Self;
    fn to_unsigned(self) -> Self::Unsigned {
        bytemuck::must_cast(self)
    }
}

macro_rules! impl_int {
    ($($int:ident => $uint:ident),+) => {
        $(
            impl Int for $int {
                type Une = [u8; core::mem::size_of::<Self>()];
                type Int = Self;
                fn with_input(ints: &mut [Self], f: impl FnOnce(&mut [Self::Int])) {
                    f(ints)
                }
                fn with_output<'a>(out: &mut CowSlice<'a, Self::Une>, _: usize, f: impl FnOnce(&mut CowSlice<'a, <Self::Int as Int>::Une>) -> Result<()>) -> Result<()> {
                    f(out)
                }
            }
            impl SizedInt for $int {
                type Unsigned = $uint;
                const MIN: Self = Self::MIN;
                const MAX: Self = Self::MAX;
            }
        )+
    }
}
impl_int!(u8 => u8, u16 => u16, u32 => u32, u64 => u64, u128 => u128);
impl_int!(i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128);

/// A [`SizedInt`] that is unsigned.
pub trait SizedUInt: SizedInt + TryInto<u128> {
    fn read(input: &mut &[u8]) -> Result<Self>;
    fn write(v: Self, out: &mut Vec<u8>);
    fn wrapping_add(self, rhs: Self::Une) -> Self::Une;
    fn wrapping_sub(self, rhs: Self) -> Self;
    fn pack128(v: &[Self], out: &mut Vec<u8>);
    fn pack64(v: &[Self], out: &mut Vec<u8>);
    fn pack32(v: &[Self], out: &mut Vec<u8>);
    fn pack16(v: &[Self], out: &mut Vec<u8>);
    fn pack8(v: &mut [Self], out: &mut Vec<u8>);
    fn unpack128<'a>(v: &'a [[u8; 16]], out: &mut CowSlice<'a, Self::Une>) -> Result<()>;
    fn unpack64<'a>(v: &'a [[u8; 8]], out: &mut CowSlice<'a, Self::Une>) -> Result<()>;
    fn unpack32<'a>(v: &'a [[u8; 4]], out: &mut CowSlice<'a, Self::Une>) -> Result<()>;
    fn unpack16<'a>(v: &'a [[u8; 2]], out: &mut CowSlice<'a, Self::Une>) -> Result<()>;
    fn unpack8<'a>(
        input: &mut &'a [u8],
        length: usize,
        out: &mut CowSlice<'a, Self::Une>,
    ) -> Result<()>;
}

macro_rules! impl_simple {
    () => {
        fn read(input: &mut &[u8]) -> Result<Self> {
            Ok(Self::from_le_bytes(consume_byte_arrays(input, 1)?[0]))
        }
        fn write(v: Self, out: &mut Vec<u8>) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        #[inline]
        fn wrapping_add(self, rhs: Self::Une) -> Self::Une {
            self.wrapping_add(Self::from_ne_bytes(rhs)).to_ne_bytes()
        }
        #[inline]
        fn wrapping_sub(self, rhs: Self) -> Self {
            self.wrapping_sub(rhs)
        }
    };
}
macro_rules! impl_unreachable {
    ($t:ty, $pack:ident, $unpack:ident) => {
        fn $pack(_: &[Self], _: &mut Vec<u8>) {
            unreachable!(); // Packings that increase size won't be chosen.
        }
        fn $unpack<'a>(_: &'a [<$t as Int>::Une], _: &mut CowSlice<'a, Self::Une>) -> Result<()> {
            unreachable!(); // Packings that increase size are unrepresentable.
        }
    };
}
macro_rules! impl_self {
    ($pack:ident, $unpack:ident) => {
        fn $pack(v: &[Self], out: &mut Vec<u8>) {
            // If we're little endian we can copy directly because we encode in little endian.
            if cfg!(target_endian = "little") {
                out.extend_from_slice(bytemuck::must_cast_slice(&v));
            } else {
                out.extend(v.iter().flat_map(|&v| v.to_le_bytes()));
            }
        }
        fn $unpack<'a>(v: &'a [Self::Une], out: &mut CowSlice<'a, Self::Une>) -> Result<()> {
            // If we're little endian we can borrow the input since we encode in little endian.
            if cfg!(target_endian = "little") {
                out.set_borrowed(v);
            } else {
                out.set_owned()
                    .extend(v.iter().map(|&v| Self::from_le_bytes(v).to_ne_bytes()));
            }
            Ok(())
        }
    };
}
macro_rules! impl_smaller {
    ($t:ty, $pack:ident, $unpack:ident) => {
        fn $pack(v: &[Self], out: &mut Vec<u8>) {
            out.extend(v.iter().flat_map(|&v| (v as $t).to_le_bytes()))
        }
        fn $unpack<'a>(v: &'a [<$t as Int>::Une], out: &mut CowSlice<'a, Self::Une>) -> Result<()> {
            out.set_owned().extend(
                v.iter()
                    .map(|&v| (<$t>::from_le_bytes(v) as Self).to_ne_bytes()),
            );
            Ok(())
        }
    };
}

// Scratch space to bridge gap between pack_ints and pack_bytes.
// In theory, we could avoid this intermediate step, but it would result in a lot of generated code.
#[cfg(feature = "std")]
fn with_scratch<T>(mut f: impl FnMut(&mut Vec<u8>) -> T) -> T {
    thread_local! {
        static SCRATCH: core::cell::RefCell<Vec<u8>> = const { core::cell::RefCell::new(Vec::new()) }
    }
    SCRATCH
        .try_with(|s| {
            let s = &mut s.borrow_mut();
            s.clear();
            f(s)
        })
        .unwrap_or_else(|_| f(&mut Vec::new()))
}
// Resort to allocation.
#[cfg(not(feature = "std"))]
fn with_scratch<T>(mut f: impl FnMut(&mut Vec<u8>) -> T) -> T {
    f(&mut Vec::new())
}

macro_rules! impl_u8 {
    () => {
        fn pack8(v: &mut [Self], out: &mut Vec<u8>) {
            with_scratch(|bytes| {
                bytes.extend(v.iter().map(|&v| v as u8));
                pack_bytes(bytes, out);
            })
        }
        fn unpack8(input: &mut &[u8], length: usize, out: &mut CowSlice<Self::Une>) -> Result<()> {
            with_scratch(|allocation| {
                // unpack_bytes might not result in a copy, but if it does we want to avoid an allocation.
                let mut bytes = CowSlice::with_allocation(core::mem::take(allocation));
                unpack_bytes(input, length, &mut bytes)?;
                // Safety: unpack_bytes ensures bytes has length of `length`.
                let slice = unsafe { bytes.as_slice(length) };
                out.set_owned()
                    .extend(slice.iter().map(|&v| (v as Self).to_ne_bytes()));
                *allocation = bytes.into_allocation();
                Ok(())
            })
        }
    };
}

impl SizedUInt for u128 {
    impl_simple!();
    impl_self!(pack128, unpack128);
    impl_smaller!(u64, pack64, unpack64);
    impl_smaller!(u32, pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl SizedUInt for u64 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_self!(pack64, unpack64);
    impl_smaller!(u32, pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl SizedUInt for u32 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_self!(pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl SizedUInt for u16 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_unreachable!(u32, pack32, unpack32);
    impl_self!(pack16, unpack16);
    impl_u8!();
}
impl SizedUInt for u8 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_unreachable!(u32, pack32, unpack32);
    impl_unreachable!(u16, pack16, unpack16);
    // Doesn't use impl_u8!() because it would copy unnecessary.
    fn pack8(v: &mut [Self], out: &mut Vec<u8>) {
        pack_bytes(v, out);
    }
    fn unpack8<'a>(
        input: &mut &'a [u8],
        length: usize,
        out: &mut CowSlice<'a, [u8; 1]>,
    ) -> Result<()> {
        unpack_bytes(input, length, out.cast_mut::<u8>())
    }
}

pub fn minmax<T: SizedInt>(v: &[T]) -> (T, T) {
    let mut min = T::MAX;
    let mut max = T::MIN;
    for &v in v.iter() {
        min = min.min(v);
        max = max.max(v);
    }
    (min, max)
}

fn skip_packing<T: SizedInt>(length: usize) -> bool {
    // Be careful using size_of::<T> since usize can be 4 or 8.
    if core::mem::size_of::<T>() == 1 {
        return true; // u8s can't be packed by pack_ints (only pack_bytes).
    }
    if length == 0 {
        return true; // Can't pack 0 ints.
    }
    // Packing a single u16 is pointless (takes at least 2 bytes).
    core::mem::size_of::<T>() == 2 && length == 1
}

/// Like [`pack_bytes`] but for larger integers. Handles endian conversion.
pub fn pack_ints<T: Int>(ints: &mut [T], out: &mut Vec<u8>) {
    T::with_input(ints, |ints| pack_ints_sized(ints, out));
}

/// [`pack_ints`] but after isize has been converted to i64.
fn pack_ints_sized<T: SizedInt>(ints: &mut [T], out: &mut Vec<u8>) {
    // Handle i8 right away since pack_bytes needs to know that it's signed.
    // If we didn't have this special case [0i8, -1, 0, -1, 0, -1] couldn't be packed.
    // Doesn't affect larger signed ints because they're made positive before pack_bytes::<u8> is called.
    if core::mem::size_of::<T>() == 1 && T::MIN < T::default() {
        let ints: &mut [i8] = bytemuck::must_cast_slice_mut(ints);
        pack_bytes(ints, out);
        return;
    };

    let (basic_packing, min_max) = if skip_packing::<T>(ints.len()) {
        (Packing::new(T::Unsigned::MAX), None)
    } else {
        // Take a small sample to avoid wastefully scanning the whole slice.
        let (sample, remaining) = ints.split_at(ints.len().min(16));
        let (min, max) = minmax(sample);

        // Only have to check packing(max - min) since it's always as good as packing(max).
        let none = Packing::new(T::Unsigned::MAX);
        if Packing::new(max.to_unsigned().wrapping_sub(min.to_unsigned())) == none {
            none.write::<T::Unsigned>(out, false);
            (none, None)
        } else {
            let (remaining_min, remaining_max) = minmax(remaining);
            let min = min.min(remaining_min);
            let max = max.max(remaining_max);

            // Signed ints pack as unsigned ints if positive.
            let basic_packing = if min >= T::default() {
                Packing::new(max.to_unsigned())
            } else {
                none // Any negative can't be packed without offset_packing.
            };
            (basic_packing, Some((min, max)))
        }
    };
    let ints = bytemuck::must_cast_slice_mut(ints);
    let min_max = min_max.map(|(min, max)| (min.to_unsigned(), max.to_unsigned()));
    pack_ints_sized_unsigned::<T::Unsigned>(ints, out, basic_packing, min_max);
}

/// [`pack_ints_sized`] but after signed integers have been cast to unsigned.
fn pack_ints_sized_unsigned<T: SizedUInt>(
    ints: &mut [T],
    out: &mut Vec<u8>,
    basic_packing: Packing,
    min_max: Option<(T, T)>,
) {
    let p = if let Some((min, max)) = min_max {
        // If subtracting min from all ints results in a better packing do it, otherwise don't bother.
        let offset_packing = Packing::new(max.wrapping_sub(min));
        if offset_packing > basic_packing && ints.len() > 5 {
            for b in ints.iter_mut() {
                *b = b.wrapping_sub(min);
            }
            offset_packing.write::<T>(out, true);
            T::write(min, out);
            offset_packing
        } else {
            basic_packing.write::<T>(out, false);
            basic_packing
        }
    } else {
        basic_packing
    };

    match p {
        Packing::_128 => T::pack128(ints, out),
        Packing::_64 => T::pack64(ints, out),
        Packing::_32 => T::pack32(ints, out),
        Packing::_16 => T::pack16(ints, out),
        Packing::_8 => T::pack8(ints, out),
    }
}

/// Opposite of [`pack_ints`]. Unpacks into `T::Une` aka unaligned native endian.
pub fn unpack_ints<'a, T: Int>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, T::Une>,
) -> Result<()> {
    T::with_output(out, length, |out| {
        unpack_ints_sized::<T::Int>(input, length, out)
    })
}

/// [`unpack_ints`] but after isize has been converted to i64.
fn unpack_ints_sized<'a, T: SizedInt>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, T::Une>,
) -> Result<()> {
    unpack_ints_sized_unsigned::<T::Unsigned>(input, length, out.cast_mut())
}

/// [`unpack_ints_sized`] but after signed integers have been cast to unsigned.
fn unpack_ints_sized_unsigned<'a, T: SizedUInt>(
    input: &mut &'a [u8],
    length: usize,
    out: &mut CowSlice<'a, T::Une>,
) -> Result<()> {
    let (p, min) = if skip_packing::<T>(length) {
        (Packing::new(T::MAX), None)
    } else {
        let (p, offset_by_min) = Packing::read::<T>(input)?;
        (p, offset_by_min.then(|| T::read(input)).transpose()?)
    };

    match p {
        Packing::_128 => T::unpack128(consume_byte_arrays(input, length)?, out),
        Packing::_64 => T::unpack64(consume_byte_arrays(input, length)?, out),
        Packing::_32 => T::unpack32(consume_byte_arrays(input, length)?, out),
        Packing::_16 => T::unpack16(consume_byte_arrays(input, length)?, out),
        Packing::_8 => T::unpack8(input, length, out),
    }?;
    if let Some(min) = min {
        // Has to be owned to have min.
        out.mut_owned(|out| {
            for v in out.iter_mut() {
                *v = min.wrapping_add(*v);
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{usize_too_big, CowSlice, Int, Result};
    use crate::error::err;
    use alloc::borrow::ToOwned;
    use alloc::vec::Vec;
    use test::{black_box, Bencher};

    pub fn pack_ints<T: Int>(ints: &[T]) -> Vec<u8> {
        let mut out = vec![];
        super::pack_ints(&mut ints.to_vec(), &mut out);
        assert_eq!(ints, unpack_ints(&out, ints.len()).unwrap());
        out
    }
    pub fn unpack_ints<T: Int>(mut packed: &[u8], length: usize) -> Result<Vec<T>> {
        let mut out = CowSlice::default();
        super::unpack_ints::<T>(&mut packed, length, &mut out)?;
        assert!(packed.is_empty());
        let unpacked = unsafe { out.as_slice(length) };
        Ok(unpacked.iter().copied().map(T::from_unaligned).collect())
    }
    const COUNTING: [usize; 8] = [0usize, 1, 2, 3, 4, 5, 6, 7];

    #[test]
    fn test_usize_eq_u64() {
        let a = COUNTING;
        let b = a.map(|v| v as u64);
        assert_eq!(pack_ints(&a), pack_ints(&b));
        let a = COUNTING.map(|v| v + 1000);
        let b = a.map(|a| a as u64);
        assert_eq!(pack_ints(&a), pack_ints(&b));
    }

    #[test]
    fn test_usize_too_big() {
        for scale in [1, 1 << 8, 1 << 16, 1 << 32] {
            let a = COUNTING.map(|v| v as u64 * scale + u32::MAX as u64);
            let packed = pack_ints(&a);
            let b = unpack_ints::<usize>(&packed, a.len());
            if cfg!(target_pointer_width = "64") {
                let b = b.unwrap();
                assert_eq!(a, core::array::from_fn(|i| b[i] as u64));
            } else {
                assert_eq!(b.unwrap_err(), usize_too_big());
            }
        }
    }

    #[test]
    fn test_isize_too_big() {
        for scale in [1, 1 << 8, 1 << 16, 1 << 32] {
            let a = COUNTING.map(|v| v as i64 * scale + i32::MAX as i64);
            let packed = pack_ints(&a);
            let b = unpack_ints::<isize>(&packed, a.len());
            if cfg!(target_pointer_width = "64") {
                let b = b.unwrap();
                assert_eq!(a, core::array::from_fn(|i| b[i] as i64));
            } else {
                assert_eq!(b.unwrap_err(), usize_too_big());
            }
        }
    }

    #[test]
    fn test_i8_special_case() {
        assert_eq!(
            pack_ints(&[0i8, -1, 0, -1, 0, -1, 0]),
            [9, (-1i8) as u8, 0b1010101]
        );
    }

    #[test]
    fn test_isize_sign_extension() {
        assert_eq!(
            pack_ints(&[0isize, -1, 0, -1, 0, -1, 0]),
            [5, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 10, 0b1010101]
        );
    }

    #[test]
    fn unpack_ints_errors() {
        assert_eq!(
            super::unpack_ints::<u16>(&mut [1].as_slice(), 5, &mut Default::default()),
            err("EOF")
        );
        assert_eq!(
            super::unpack_ints::<u16>(&mut [255].as_slice(), 5, &mut Default::default()),
            super::invalid_packing()
        );
    }

    fn test_inner<T: Int>(ints: &[T]) -> Vec<u8> {
        let out = pack_ints(&mut ints.to_owned());
        let unpacked = unpack_ints::<T>(&out, ints.len()).unwrap();
        assert_eq!(unpacked, ints);
        #[cfg(feature = "std")]
        {
            let packing = out[0];
            let size = 100.0 * out.len() as f32 / core::mem::size_of_val(ints) as f32;
            println!("{packing} {size:>5.1}%");
        }
        out
    }

    #[rustfmt::skip]
    macro_rules! test {
        ($name:ident, $t:ty) => {
            #[test]
            fn $name() {
                type T = $t;
                for increment in [0, 1, u8::MAX as u128 + 1, u16::MAX as u128 + 1, u32::MAX as u128 + 1, u64::MAX as u128 + 1] {
                    #[allow(irrefutable_let_patterns)]
                    let Ok(increment) = T::try_from(increment) else {
                        continue;
                    };

                    for max in [
                        i128::MIN, i64::MIN as i128, i32::MIN as i128, i16::MIN as i128, i8::MIN as i128, -1,
                        0, i8::MAX as i128, i16::MAX as i128, i32::MAX as i128, i64::MAX as i128, i128::MAX
                    ] {
                        if max == T::MAX as i128 {
                            continue;
                        }
                        #[allow(irrefutable_let_patterns)]
                        let Ok(start) = T::try_from(max) else {
                            continue;
                        };
                        #[cfg(feature = "std")]
                        let s = format!("{start} {increment}");
                        if increment == 1 {
                            #[cfg(feature = "std")]
                            print!("{s:<19} mod 2 => ");
                            test_inner::<T>(&core::array::from_fn::<_, 100, _>(|i| {
                                start + (i as T % 2) * increment
                            }));
                        }
                        #[cfg(feature = "std")]
                        print!("{s:<25} => ");
                        test_inner::<T>(&core::array::from_fn::<_, 100, _>(|i| {
                            start + i as T * increment
                        }));
                    }
                }
            }
        };
    }
    test!(test_u008, u8);
    test!(test_u016, u16);
    test!(test_u032, u32);
    test!(test_u064, u64);
    test!(test_u128, u128);
    test!(test_usize, usize);
    test!(test_i008, i8);
    test!(test_i016, i16);
    test!(test_i032, i32);
    test!(test_i064, i64);
    test!(test_i128, i128);
    test!(test_isize, isize);

    fn bench_pack_ints<T: Int>(b: &mut Bencher, src: &[T]) {
        let mut ints = src.to_vec();
        let mut out = Vec::with_capacity(core::mem::size_of_val(src) + 10);
        let starting_cap = out.capacity();
        b.iter(|| {
            ints.copy_from_slice(&src);
            out.clear();
            super::pack_ints(black_box(&mut ints), black_box(&mut out));
        });
        assert_eq!(out.capacity(), starting_cap);
    }

    fn bench_unpack_ints<T: Int>(b: &mut Bencher, src: &[T]) {
        let packed = pack_ints(&mut src.to_vec());
        let mut out = CowSlice::with_allocation(Vec::<T::Une>::with_capacity(src.len()));
        b.iter(|| {
            let length = src.len();
            super::unpack_ints::<T>(
                black_box(&mut packed.as_slice()),
                length,
                black_box(&mut out),
            )
            .unwrap();
            debug_assert_eq!(
                unsafe { out.as_slice(length) }
                    .iter()
                    .copied()
                    .map(T::from_unaligned)
                    .collect::<Vec<_>>(),
                src
            );
        });
    }

    macro_rules! bench {
        ($name:ident, $t:ident) => {
            paste::paste! {
                #[bench]
                fn [<bench_pack_ $name _zero>](b: &mut Bencher) {
                    bench_pack_ints::<$t>(b, &[0; 1000]);
                }

                #[bench]
                fn [<bench_pack_ $name _max>](b: &mut Bencher) {
                    bench_pack_ints::<$t>(b, &[$t::MAX; 1000]);
                }

                #[bench]
                fn [<bench_pack_ $name _random>](b: &mut Bencher) {
                    bench_pack_ints::<$t>(b, &crate::random_data(1000));
                }

                #[bench]
                fn [<bench_pack_ $name _no_pack>](b: &mut Bencher) {
                    let src = vec![$t::MIN; 1000];
                    let mut ints = src.clone();
                    let mut out: Vec<u8> = Vec::with_capacity(core::mem::size_of_val(&ints) + 10);
                    b.iter(|| {
                        ints.copy_from_slice(&src);
                        let input = black_box(&mut ints);
                        out.clear();
                        let out = black_box(&mut out);
                        out.extend_from_slice(bytemuck::must_cast_slice(&input));
                    });
                }

                #[bench]
                fn [<bench_unpack_ $name _zero>](b: &mut Bencher) {
                    bench_unpack_ints::<$t>(b, &[0; 1000]);
                }

                #[bench]
                fn [<bench_unpack_ $name _max>](b: &mut Bencher) {
                    bench_unpack_ints::<$t>(b, &[$t::MAX; 1000]);
                }

                #[bench]
                fn [<bench_unpack_ $name _random>](b: &mut Bencher) {
                    bench_unpack_ints::<$t>(b, &crate::random_data(1000));
                }
            }
        };
    }
    bench!(u008, u8);
    bench!(u016, u16);
    bench!(u032, u32);
    bench!(u064, u64);
    bench!(u128, u128);
    bench!(usize, usize);
}
