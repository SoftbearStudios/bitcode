use crate::encoding::{Encoding, Fixed};
use crate::read::Read;
use crate::write::Write;
use crate::Result;

pub(crate) fn encode_internal<'a>(
    writer: &'a mut (impl Write + Default),
    t: &(impl Encode + ?Sized),
) -> Result<&'a [u8]> {
    writer.start_write();
    t.encode(Fixed, writer)?;
    Ok(writer.finish_write())
}

pub(crate) fn decode_internal<'a, T: Decode>(
    reader: &mut (impl Read + Default),
    bytes: &[u8],
) -> Result<T> {
    reader.start_read(bytes);
    let decode_result = T::decode(Fixed, reader);
    reader.finish_read_with_result(decode_result)
}

/// A type which can be encoded to bytes with [`encode`][`crate::encode`].
///
/// Must use `#[derive(Encode)]` to implement.
/// ```
/// #[derive(bitcode::Encode)]
/// struct MyStruct {
///     a: u32,
///     b: bool,
///     // If you want to use serde::Serialize on a field instead of bitcode::Encode.
///     #[cfg(feature = "serde")]
///     #[bitcode(with_serde)]
///     c: String,
/// }
/// ```
pub trait Encode {
    #[doc(hidden)]
    fn encode(&self, encoding: impl Encoding, writer: &mut impl Write) -> Result<()>;
}

/// A type which can be decoded from bytes with [`decode`][`crate::decode`].
///
/// Must use `#[derive(Decode)]` to implement.
/// ```
/// #[derive(bitcode::Decode)]
/// struct MyStruct {
///     a: u32,
///     b: bool,
///     // If you want to use serde::Deserialize on a field instead of bitcode::Decode.
///     #[cfg(feature = "serde")]
///     #[bitcode(with_serde)]
///     c: String,
/// }
/// ```
pub trait Decode: Sized {
    #[doc(hidden)]
    fn decode(encoding: impl Encoding, reader: &mut impl Read) -> Result<Self>;
}
