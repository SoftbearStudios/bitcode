#[cfg(debug_assertions)]
use alloc::borrow::Cow;
use core::fmt::{Debug, Display, Formatter};

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
    return Error(Cow::Owned(alloc::string::ToString::to_string(&_t)));
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
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        #[cfg(debug_assertions)]
        return write!(f, "Error({:?})", self.0);
        #[cfg(not(debug_assertions))]
        f.write_str("Error(\"bitcode error\")")
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        #[cfg(debug_assertions)]
        return f.write_str(&self.0);
        #[cfg(not(debug_assertions))]
        f.write_str("bitcode error")
    }
}
#[cfg(feature = "std")]
// TODO expose to no_std when error_in_core stabilized (https://github.com/rust-lang/rust/issues/103765)
impl std::error::Error for Error {}
