use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use alloc::vec::Vec;
use core::num::NonZeroUsize;
use core::ops::Deref;

pub struct DerefEncoder<T: Encode + ?Sized>(T::Encoder);

// Can't derive since it would bound T: Default.
impl<T: Encode + ?Sized> Default for DerefEncoder<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<D: Deref<Target = T>, T: Encode + ?Sized> Encoder<D> for DerefEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &D) {
        self.0.encode(t);
    }
}

impl<T: Encode + ?Sized> Buffer for DerefEncoder<T> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }
    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

/// Decodes a `T` and then converts it with [`From`]. For example `T` -> `Box<T>` and `Vec<T>` -> `Box<[T]>`.
pub struct FromDecoder<'a, T: Decode<'a>>(T::Decoder);

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>> Default for FromDecoder<'a, T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<'a, T: Decode<'a>> View<'a> for FromDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)
    }
}

impl<'a, F: From<T>, T: Decode<'a>> Decoder<'a, F> for FromDecoder<'a, T> {
    #[inline(always)]
    fn decode(&mut self) -> F {
        F::from(self.0.decode())
    }
}

#[cfg(test)]
mod tests {
    use crate::{decode, encode};
    use alloc::boxed::Box;
    use alloc::string::ToString;

    #[test]
    fn box_() {
        let v = Box::new(123u8);
        assert_eq!(decode::<Box<u8>>(&encode(&v)).unwrap(), v);
    }

    #[test]
    fn box_slice() {
        let v = vec![123u8].into_boxed_slice();
        assert_eq!(decode::<Box<[u8]>>(&encode(&v)).unwrap(), v);
    }

    #[test]
    fn box_str() {
        let v = "box".to_string().into_boxed_str();
        assert_eq!(decode::<Box<str>>(&encode(&v)).unwrap(), v);
    }
}
