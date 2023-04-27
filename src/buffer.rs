use crate::word_buffer::WordBuffer;

/// A buffer for reusing allocations between any number of calls to [`Buffer::encode`] and/or
/// [`Buffer::decode`].
///
/// ### Usage
/// ```edition2021
/// use bitcode::Buffer;
///
/// // We preallocate buffers with capacity 1000. This will allow us to encode and decode without
/// // any allocations as long as the encoded object takes less than 1000 bytes.
/// let bytes = 1000;
/// let mut encode_buf = Buffer::with_capacity(bytes);
/// let mut decode_buf = Buffer::with_capacity(bytes);
///
/// // The object that we will encode.
/// let target: [u8; 5] = [1, 2, 3, 4, 5];
///
/// // We encode into `encode_buf`. This won't cause any allocations.
/// let encoded: &[u8] = encode_buf.encode(&target).unwrap();
/// assert!(encoded.len() <= bytes, "oh no we allocated");
///
/// // We decode into `decode_buf` because `encoded` is borrowing `encode_buf`.
/// let decoded: [u8; 5] = decode_buf.decode(&encoded).unwrap();
/// assert_eq!(target, decoded);
///
/// // If we need ownership of `encoded`, we can convert it to a vec.
/// // This will allocate, but it's still more efficient than calling bitcode::encode.
/// let _owned: Vec<u8> = encoded.to_vec();
/// ```
#[derive(Default)]
pub struct Buffer(pub(crate) WordBuffer);

impl Buffer {
    /// Constructs a new buffer without any capacity.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a new buffer with at least the specified capacity in bytes.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(WithCapacity::with_capacity(capacity))
    }

    /// Returns the capacity in bytes.
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

pub trait WithCapacity {
    fn capacity(&self) -> usize;
    fn with_capacity(capacity: usize) -> Self;
}

#[cfg(all(test, not(miri), debug_assertions))]
mod tests {
    use crate::bit_buffer::BitBuffer;
    use crate::buffer::WithCapacity;
    use crate::word_buffer::WordBuffer;
    use paste::paste;

    macro_rules! test_with_capacity {
        ($name:ty, $t:ty) => {
            paste! {
                #[test]
                fn [<test_ $name _with_capacity>]() {
                    for cap in 0..200 {
                        let buf = $t::with_capacity(cap);
                        assert!(buf.capacity() >= cap, "with_capacity: {cap}, capacity {}", buf.capacity());
                    }
                }
            }
        }
    }

    test_with_capacity!(bit_buffer, BitBuffer);
    test_with_capacity!(word_buffer, WordBuffer);
}
