use crate::error::{error_from_display, Error};
use std::fmt::Display;

mod de;
mod guard;
mod ser;
mod variant;

pub use de::*;
pub use ser::*;

// Use macro instead of function because ! type isn't stable.
macro_rules! type_changed {
    () => {
        panic!("type changed")
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
