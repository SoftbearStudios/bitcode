/// `#![feature(int_roundings)]` was stabilized in 1.73, but we want to avoid MSRV that high.
macro_rules! impl_div_ceil {
    ($name:ident, $t:ty) => {
        #[inline(always)]
        #[allow(unused_comparisons)] // < 0 checks not required for unsigned
        pub const fn $name(lhs: $t, rhs: $t) -> $t {
            let d = lhs / rhs;
            let r = lhs % rhs;
            if (r > 0 && rhs > 0) || (r < 0 && rhs < 0) {
                d + 1
            } else {
                d
            }
        }
    };
}
impl_div_ceil!(div_ceil_u8, u8);
impl_div_ceil!(div_ceil_usize, usize);
