use crate::error::{error_from_display, Error};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Display;

mod de;
mod guard;
mod ser;
mod variant;

pub use de::*;
pub use ser::*;

// Saves code size over calling panic in type_changed macro directly.
enum Never {}
#[cold]
#[inline(never)]
fn panic_type_changed() -> Never {
    panic!("type changed")
}
macro_rules! type_changed {
    () => {
        #[allow(unreachable_code)]
        match super::panic_type_changed() {} // ! is unstable.
    };
}
use type_changed;

fn default_box_slice<T: Default>(len: usize) -> Box<[T]> {
    let mut vec = vec![];
    vec.resize_with(len, Default::default);
    vec.into()
}

#[inline(always)]
fn get_mut_or_resize<T: Default>(vec: &mut Vec<T>, index: usize) -> &mut T {
    if index >= vec.len() {
        #[cold]
        #[inline(never)]
        fn cold<T: Default>(vec: &mut Vec<T>, index: usize) {
            vec.resize_with(index + 1, Default::default);
        }
        cold(vec, index);
    }
    // Safety we've just resized `vec.len()` to be > than `index`.
    unsafe { vec.get_unchecked_mut(index) }
}

#[cfg(not(feature = "std"))]
impl serde::ser::StdError for Error {}

impl serde::ser::Error for Error {
    fn custom<T>(t: T) -> Self
    where
        T: Display,
    {
        error_from_display(t)
    }
}

impl serde::de::Error for Error {
    fn custom<T>(t: T) -> Self
    where
        T: Display,
    {
        error_from_display(t)
    }
}
