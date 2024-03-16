#[cfg(debug_assertions)]
use std::borrow::Cow;
use std::fmt::{Debug, Display, Formatter};

/// Short version of `Err(error("..."))`.
pub fn err<T>(msg: &'static str) -> Result<T, Error> {
    Err(error(msg))
}

/// Creates an error with a message that might be displayed.
pub fn error(_msg: &'static str) -> Error {
    #[cfg(debug_assertions)]
    return Error(Cow::Borrowed(_msg));
    #[cfg(not(debug_assertions))]
    Error(())
}

/// Creates an error from a `T:` [`Display`].
#[cfg(feature = "serde")]
pub fn error_from_display(_t: impl Display) -> Error {
    #[cfg(debug_assertions)]
    return Error(Cow::Owned(_t.to_string()));
    #[cfg(not(debug_assertions))]
    Error(())
}

#[cfg(debug_assertions)]
type ErrorImpl = Cow<'static, str>;
#[cfg(not(debug_assertions))]
type ErrorImpl = ();

/// Decoding / (De)serialization errors.
/// # Debug mode
/// In debug mode, the error contains a reason.
/// # Release mode
/// In release mode, the error is a zero-sized type for efficiency.
#[cfg_attr(test, derive(PartialEq))]
pub struct Error(ErrorImpl);
impl Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error({:?})", self.to_string())
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        #[cfg(debug_assertions)]
        return f.write_str(&self.0);
        #[cfg(not(debug_assertions))]
        f.write_str("bitcode error")
    }
}
impl std::error::Error for Error {}
