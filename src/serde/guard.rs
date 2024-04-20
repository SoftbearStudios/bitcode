use crate::coder::Result;
use crate::error::err;

pub const ZST_LIMIT: usize = 1 << 16;

fn check_zst_len(len: usize) -> Result<()> {
    if len > ZST_LIMIT {
        err("too many zero sized types")
    } else {
        Ok(())
    }
}

// Used by deserialize. Guards against Vec<()> with huge len taking forever.
#[inline]
pub fn guard_zst<T>(len: usize) -> Result<()> {
    if core::mem::size_of::<T>() == 0 {
        check_zst_len(len)
    } else {
        Ok(())
    }
}
