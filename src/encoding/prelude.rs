pub use crate::encoding::Encoding;
pub use crate::nightly::ilog2_u64;
pub use crate::read::Read;
pub use crate::word::*;
pub use crate::write::Write;
pub(crate) use crate::{Result, E};

#[cfg(all(debug_assertions, test))]
pub mod test_prelude {
    pub use super::*;
    pub use crate::{Decode, Encode};
    pub use std::fmt::Debug;

    pub fn test_encoding_inner<
        B: Read + Write + Default,
        V: Encode + Decode + Debug + PartialEq,
    >(
        encoding: impl Encoding,
        value: V,
    ) {
        let mut buffer = B::default();

        buffer.start_write();
        value.encode(encoding, &mut buffer).unwrap();
        let bytes = buffer.finish_write().to_owned();

        buffer.start_read(&bytes);
        assert_eq!(V::decode(encoding, &mut buffer).unwrap(), value);
        buffer.finish_read().unwrap();
    }

    pub fn test_encoding<V: Encode + Decode + Copy + Debug + PartialEq>(
        encoding: impl Encoding,
        value: V,
    ) {
        test_encoding_inner::<crate::bit_buffer::BitBuffer, V>(encoding, value);
        test_encoding_inner::<crate::word_buffer::WordBuffer, V>(encoding, value);
    }
}
