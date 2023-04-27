use crate::read::Read;
use crate::{Result, E};

pub const ZST_LIMIT: usize = 1 << 16;

// Guards against Vec<()> with huge len taking forever.
#[inline]
pub fn guard_zst<T>(len: usize) -> Result<()> {
    if std::mem::size_of::<T>() == 0 && len > ZST_LIMIT {
        Err(E::Invalid("too many zst").e())
    } else {
        Ok(())
    }
}

// Guards against allocating huge Vec<T> without enough remaining bits to fill it.
// Also calls [`guard_zst`].
#[inline]
pub fn guard_len<T>(len: usize, reader: &impl Read) -> Result<()> {
    if std::mem::size_of::<T>() == 0 {
        guard_zst::<T>(len)
    } else {
        // We assume that each non zero sized T requires at least 1 bit. If it took 0 bits
        // decoding could allocate unbounded memory.
        // TODO could multiply by min bits per T if we knew that.
        let bits = len * 1;
        reader.reserve_bits(bits)
    }
}
