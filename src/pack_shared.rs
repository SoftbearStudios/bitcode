use crate::pack_ints::{SizedInt, SizedUInt};
use alloc::vec::Vec;

pub trait PackingTrait: Copy + PartialOrd {
    fn new<T: SizedUInt>(max: T) -> Self;

    fn write<T: SizedUInt>(self, out: &mut Vec<u8>, offset_by_min: bool);
}

fn minmax<T: SizedInt>(v: &[T]) -> (T, T) {
    let mut min = T::MAX;
    let mut max = T::MIN;
    for &v in v.iter() {
        min = min.min(v);
        max = max.max(v);
    }
    (min, max)
}

// Writes a packing to `out` iff it returns None.
pub fn basic_packing_and_signed_min_max_cast_to_unsigned<T: SizedInt, P: PackingTrait>(
    ints: &[T],
    out: &mut Vec<u8>,
) -> (P, Option<(T::Unsigned, T::Unsigned)>) {
    // Take a small sample to avoid wastefully scanning the whole slice.
    // Note: This small sample is purely an optimization, it has no impact on the encoded result
    // because we only use it to bail from scanning the entire slice if the first 16-32 elements
    // cannot be packed.
    let sample_size = (32 / core::mem::size_of::<T>()).max(16);
    let (sample, remaining) = ints.split_at(ints.len().min(sample_size));
    let (min, max) = minmax(sample);

    // Only have to check packing(max - min) since it's always as good as packing(max).
    let none = P::new(T::Unsigned::MAX);
    if P::new(max.to_unsigned().wrapping_sub(min.to_unsigned())) == none {
        none.write::<T::Unsigned>(out, false);
        (none, None)
    } else {
        let (remaining_min, remaining_max) = minmax(remaining);
        let min = min.min(remaining_min);
        let max = max.max(remaining_max);

        // Signed ints pack as unsigned ints if positive.
        let basic_packing = if min >= T::default() {
            P::new(max.to_unsigned())
        } else {
            none // Any negative can't be packed without offset_packing.
        };

        (basic_packing, Some((min.to_unsigned(), max.to_unsigned())))
    }
}

// Writes a packing to `out` iff `min_max` is Some.
pub fn offset_packing<T: SizedUInt, P: PackingTrait>(
    ints: &mut [T],
    out: &mut Vec<u8>,
    basic_packing: P,
    min_max: Option<(T, T)>,
) -> P {
    if let Some((min, max)) = min_max {
        // If subtracting min from all ints results in a better packing do it, otherwise don't bother.
        let offset_packing = P::new(max.wrapping_sub(min));
        // TODO(breaking change) don't hardcode this as 5. Only perform offset_packing
        // on a few elements if the added T::write(min, out) makes it still smaller.
        let small_skip_offset_packing = 5;
        if offset_packing > basic_packing && ints.len() > small_skip_offset_packing {
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
    }
}
