#[cfg(any(
    target_has_atomic = "8",
    target_has_atomic = "16",
    target_has_atomic = "32",
    target_has_atomic = "64",
    target_has_atomic = "ptr"
))]
use core::sync::atomic::Ordering::Relaxed;
#[cfg(target_has_atomic = "8")]
use core::sync::atomic::{AtomicBool, AtomicI8, AtomicU8};
#[cfg(target_has_atomic = "16")]
use core::sync::atomic::{AtomicI16, AtomicU16};
#[cfg(target_has_atomic = "32")]
use core::sync::atomic::{AtomicI32, AtomicU32};
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::{AtomicI64, AtomicU64};
#[cfg(target_has_atomic = "ptr")]
use core::sync::atomic::{AtomicIsize, AtomicUsize};

macro_rules! atomic_impl {
    ($(($atomic: path, $repr: ident, $size:expr),)*) => {
        $(
        #[cfg(target_has_atomic = $size)]
        impl super::convert::ConvertFrom<&$atomic> for $repr {
            #[inline(always)]
            fn convert_from(atomic: &$atomic) -> Self {
                // `Relaxed` matches `Debug` and `serde::Serialize`. It is your responsiblity to avoid
                // race conditions, such as by excluding or fencing operations from other threads.
                atomic.load(Relaxed)
            }
        }
        #[cfg(target_has_atomic = $size)]
        impl super::convert::ConvertFrom<$repr> for $atomic {
            #[inline(always)]
            fn convert_from(bits: $repr) -> Self {
                Self::from(bits)
            }
        }
        #[cfg(target_has_atomic = $size)]
        impl crate::derive::Encode for $atomic {
            type Encoder = crate::derive::convert::ConvertIntoEncoder<$repr>;
        }
        #[cfg(target_has_atomic = $size)]
        impl<'a> crate::derive::Decode<'a> for $atomic {
            type Decoder = crate::derive::convert::ConvertFromDecoder<'a, $repr>;
        }
        )*
    };
}

atomic_impl!(
    (AtomicBool, bool, "8"),
    (AtomicI8, i8, "8"),
    (AtomicI16, i16, "16"),
    (AtomicI32, i32, "32"),
    (AtomicI64, i64, "64"),
    (AtomicIsize, isize, "ptr"),
    (AtomicU8, u8, "8"),
    (AtomicU16, u16, "16"),
    (AtomicU32, u32, "32"),
    (AtomicUsize, usize, "ptr"),
    (AtomicU64, u64, "64"),
);

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use core::sync::atomic::*;

    #[test]
    fn test_atomic() {
        macro_rules! atomic_test {
            ($atomic: ident, $inner: expr) => {
                assert!(decode::<$atomic>(&encode(&$atomic::new($inner)))
                    .is_ok_and(|x| x.load(Ordering::Relaxed) == $inner));
            };
        }
        atomic_test!(AtomicBool, true);
        atomic_test!(AtomicI8, -128i8);
        atomic_test!(AtomicI16, -5897i16);
        atomic_test!(AtomicI32, -487952i32);
        atomic_test!(AtomicI64, -783414i64);
        atomic_test!(AtomicIsize, isize::MIN);
        atomic_test!(AtomicU8, 255u8);
        atomic_test!(AtomicU16, 8932u16);
        atomic_test!(AtomicU32, 58902u32);
        atomic_test!(AtomicU64, 90887783u64);
        atomic_test!(AtomicUsize, usize::MAX);
    }
}
