use crate::coder::Result;
use crate::error::{err, error};

/// Attempts to claim `bytes` bytes out of `input`.
pub fn consume_bytes<'a>(input: &mut &'a [u8], bytes: usize) -> Result<&'a [u8]> {
    if bytes > input.len() {
        return err("EOF");
    }
    let (bytes, remaining) = input.split_at(bytes);
    *input = remaining;
    Ok(bytes)
}

/// Attempts to claim one byte out of `input`.
pub fn consume_byte(input: &mut &[u8]) -> Result<u8> {
    Ok(consume_bytes(input, 1)?[0])
}

/// Like `consume_bytes` but consumes `[u8; N]` instead of `u8`.
pub fn consume_byte_arrays<'a, const N: usize>(
    input: &mut &'a [u8],
    length: usize,
) -> Result<&'a [[u8; N]]> {
    // Avoid * overflow by using / instead.
    if input.len() / N < length {
        return err("EOF");
    }

    // Safety: input.len() >= mid since we've checked it above.
    let mid = length * N;
    let (bytes, remaining) = unsafe { (input.get_unchecked(..mid), input.get_unchecked(mid..)) };

    *input = remaining;
    Ok(bytemuck::cast_slice(bytes))
}

/// Check if `input` is empty or return error.
pub fn expect_eof(input: &[u8]) -> Result<()> {
    if cfg!(not(fuzzing)) && !input.is_empty() {
        err("Expected EOF")
    } else {
        Ok(())
    }
}

/// Returns `Ok(length * x)` if it does not overflow.
pub fn mul_length(length: usize, x: usize) -> Result<usize> {
    length
        .checked_mul(x)
        .ok_or_else(|| error("length overflow"))
}
