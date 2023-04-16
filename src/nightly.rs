// Replacements for nightly features used while developing the crate.

#[inline]
pub fn div_ceil(me: usize, rhs: usize) -> usize {
    let d = me / rhs;
    let r = me % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}

#[inline]
pub fn ilog2(me: usize) -> u32 {
    assert_ne!(me, 0);
    usize::BITS - 1 - me.leading_zeros()
}

#[inline]
pub fn utf8_char_width(b: u8) -> usize {
    if b < 128 {
        1
    } else if b < 224 {
        2
    } else if b < 240 {
        3
    } else {
        4
    }
}
