use crate::coder::Result;
use crate::consume::{consume_byte, consume_byte_arrays};
use crate::fast::CowSlice;
use crate::pack::{invalid_packing, pack_bytes, unpack_bytes};
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
    fn new<T: Int>(max: T) -> Self {
        let max: u128 = max.into();
        #[allow(clippy::match_overlapping_arm)] // Just make sure not to reorder them.
        match max {
            ..=0xFF => Self::_8,
            ..=0xFF_FF => Self::_16,
            ..=0xFF_FF_FF_FF => Self::_32,
            ..=0xFF_FF_FF_FF_FF_FF_FF_FF => Self::_64,
            _ => Self::_128,
        }
    }

    fn write<T: Int>(self, out: &mut Vec<u8>, offset_by_min: bool) {
        // Encoded in such a way such that 0 is no packing and higher numbers are smaller packing.
        // Also makes no packing with offset_by_min = true is unrepresentable.
        out.push((self as u8 - Self::new(T::MAX) as u8) * 2 - offset_by_min as u8);
    }

    fn read<T: Int>(input: &mut &[u8]) -> Result<(Self, bool)> {
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

// Default bound makes #[derive(Default)] on IntEncoder/IntDecoder work.
pub trait Int:
    Copy + Default + Into<u128> + Ord + Pod + Sized + std::ops::Sub<Output = Self> + std::ops::SubAssign
{
    // Unaligned native endian. TODO could be aligned on big endian since we always have to copy.
    type Une: Pod + Default;
    const MIN: Self;
    const MAX: Self;
    fn read(input: &mut &[u8]) -> Result<Self>;
    fn write(v: Self, out: &mut Vec<u8>);
    fn wrapping_add(self, rhs: Self::Une) -> Self::Une;
    #[cfg(test)]
    fn from_unaligned(unaligned: Self::Une) -> Self;
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
        type Une = [u8; std::mem::size_of::<Self>()];
        const MIN: Self = Self::MIN;
        const MAX: Self = Self::MAX;
        fn read(input: &mut &[u8]) -> Result<Self> {
            Ok(Self::from_le_bytes(consume_byte_arrays(input, 1)?[0]))
        }
        fn write(v: Self, out: &mut Vec<u8>) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        fn wrapping_add(self, rhs: Self::Une) -> Self::Une {
            self.wrapping_add(Self::from_ne_bytes(rhs)).to_ne_bytes()
        }
        #[cfg(test)]
        fn from_unaligned(unaligned: Self::Une) -> Self {
            Self::from_ne_bytes(unaligned)
        }
    };
}
macro_rules! impl_unreachable {
    ($t:ty, $pack:ident, $unpack:ident) => {
        fn $pack(_: &[Self], _: &mut Vec<u8>) {
            unimplemented!();
        }
        fn $unpack<'a>(_: &'a [<$t as Int>::Une], _: &mut CowSlice<'a, Self::Une>) -> Result<()> {
            invalid_packing()
        }
    };
}
macro_rules! impl_self {
    ($pack:ident, $unpack:ident) => {
        fn $pack(v: &[Self], out: &mut Vec<u8>) {
            // If we're little endian we can copy directly because we encode in little endian.
            if cfg!(target_endian = "little") {
                out.extend_from_slice(bytemuck::cast_slice(&v));
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
fn with_scratch<T>(f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
    thread_local! {
        static SCRATCH: std::cell::RefCell<Vec<u8>> = Default::default();
    }
    SCRATCH.with(|s| {
        let s = &mut s.borrow_mut();
        s.clear();
        f(s)
    })
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
                let mut bytes = CowSlice::with_allocation(std::mem::take(allocation));
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

impl Int for u128 {
    impl_simple!();
    impl_self!(pack128, unpack128);
    impl_smaller!(u64, pack64, unpack64);
    impl_smaller!(u32, pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl Int for u64 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_self!(pack64, unpack64);
    impl_smaller!(u32, pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl Int for u32 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_self!(pack32, unpack32);
    impl_smaller!(u16, pack16, unpack16);
    impl_u8!();
}
impl Int for u16 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_unreachable!(u32, pack32, unpack32);
    impl_self!(pack16, unpack16);
    impl_u8!();
}
impl Int for u8 {
    impl_simple!();
    impl_unreachable!(u128, pack128, unpack128);
    impl_unreachable!(u64, pack64, unpack64);
    impl_unreachable!(u32, pack32, unpack32);
    impl_unreachable!(u16, pack16, unpack16);
    // Doesn't use impl_u8!() because it would copy unnecessary.
    fn pack8(v: &mut [Self], out: &mut Vec<u8>) {
        pack_bytes(v, out);
    }
    fn unpack8(input: &mut &[u8], length: usize, out: &mut CowSlice<[u8; 1]>) -> Result<()> {
        // Safety: [u8; 1] and u8 are the same from the perspective of CowSlice.
        let out: &mut CowSlice<u8> = unsafe { std::mem::transmute(out) };
        unpack_bytes(input, length, out)
    }
}

fn minmax<T: Int>(v: &[T]) -> (T, T) {
    let mut min = T::MAX;
    let mut max = T::MIN;
    for &v in v.iter() {
        min = min.min(v);
        max = max.max(v);
    }
    (min, max)
}

/// Like [`pack_bytes`] but for larger integers. Handles endian conversion.
pub fn pack_ints<T: Int>(ints: &mut [T], out: &mut Vec<u8>) {
    // Passes through u8s and length <= 1 since they can't be compressed.
    let p = if std::mem::size_of::<T>() == 1 || ints.len() <= 1 {
        Packing::new(T::MAX)
    } else {
        // Take a small sample to avoid wastefully scanning the whole slice.
        let (sample, remaining) = ints.split_at(ints.len().min(16));
        let (min, max) = minmax(sample);

        // Only have to check packing(max - min) since it's always as good as just packing(max).
        let none = Packing::new(T::MAX);
        if Packing::new(max - min) == none {
            none.write::<T>(out, false);
            none
        } else {
            let (remaining_min, remaining_max) = minmax(remaining);
            let min = min.min(remaining_min);
            let max = max.max(remaining_max);

            // If subtracting min from all ints results in a better packing do it, otherwise don't bother.
            // TODO ensure packing never expands data unnecessarily.
            let p = Packing::new(max);
            let p2 = Packing::new(max - min);
            if p2 > p && ints.len() > 5 {
                for b in ints.iter_mut() {
                    *b -= min;
                }
                p2.write::<T>(out, true);
                T::write(min, out);
                p2
            } else {
                p.write::<T>(out, false);
                p
            }
        }
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
    // Passes through u8s and length <= 1 since they can't be compressed.
    let (p, min) = if std::mem::size_of::<T>() == 1 || length <= 1 {
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
            for v in out {
                *v = min.wrapping_add(*v);
            }
        })
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{pack_ints, unpack_ints, CowSlice, Int};
    use std::fmt::Debug;
    use test::{black_box, Bencher};

    fn t<T: Int + Debug>(ints: &[T]) -> Vec<u8> {
        let mut out = vec![];
        pack_ints(&mut ints.to_owned(), &mut out);

        let mut slice = out.as_slice();
        let mut unpacked = CowSlice::default();
        let length = ints.len();
        unpack_ints::<T>(&mut slice, length, &mut unpacked).unwrap();
        let unpacked = unsafe { unpacked.as_slice(length) };
        let unpacked: Vec<_> = unpacked.iter().copied().map(T::from_unaligned).collect();
        assert_eq!(unpacked, ints);
        assert!(slice.is_empty());

        let packing = out[0];
        let size = 100.0 * out.len() as f32 / std::mem::size_of_val(ints) as f32;
        println!("{packing} {size:>5.1}%");
        out
    }

    #[rustfmt::skip]
    macro_rules! test {
        ($name:ident, $t:ty) => {
            #[test]
            fn $name() {
                type T = $t;
                for increment in [0, 1, u8::MAX as u128 + 1, u16::MAX as u128 + 1, u32::MAX as u128 + 1, u64::MAX as u128 + 1] {
                    let Ok(increment) = T::try_from(increment) else {
                        continue;
                    };

                    for max in [0, u8::MAX as u128, u16::MAX as u128, u32::MAX as u128, u64::MAX as u128, u128::MAX as u128] {
                        let Ok(start) = T::try_from(max / 2) else {
                            continue;
                        };
                        let s = format!("{start} {increment}");
                        print!("{s:<25} => ");
                        t::<T>(&std::array::from_fn::<_, 100, _>(|i| {
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

    fn bench_pack_ints<T: Int>(b: &mut Bencher, src: &[T]) {
        let mut ints = src.to_vec();
        let mut out = Vec::with_capacity(std::mem::size_of_val(src) + 10);
        let starting_cap = out.capacity();
        b.iter(|| {
            ints.copy_from_slice(&src);
            out.clear();
            pack_ints(black_box(&mut ints), black_box(&mut out));
        });
        assert_eq!(out.capacity(), starting_cap);
    }

    fn bench_unpack_ints<T: Int + Debug>(b: &mut Bencher, src: &[T]) {
        let mut packed = vec![];
        pack_ints(&mut src.to_vec(), &mut packed);
        let mut out = CowSlice::with_allocation(Vec::<T::Une>::with_capacity(src.len()));
        b.iter(|| {
            let length = src.len();
            unpack_ints::<T>(
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
                    let mut out: Vec<u8> = Vec::with_capacity(std::mem::size_of_val(&ints) + 10);
                    b.iter(|| {
                        ints.copy_from_slice(&src);
                        let input = black_box(&mut ints);
                        out.clear();
                        let out = black_box(&mut out);
                        out.extend_from_slice(bytemuck::cast_slice(&input));
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
}
