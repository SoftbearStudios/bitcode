use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::Result;

/// A writer that can only hold 64 bits, but only uses registers instead of load/store.
pub struct RegisterWriter<'a, W: Write> {
    pub writer: &'a mut W,
    pub inner: RegisterBuffer,
}

impl<'a, W: Write> RegisterWriter<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            inner: Default::default(),
        }
    }
}

impl<'a, W: Write> RegisterWriter<'a, W> {
    /// Writes the contents of the buffer to `writer` and clears the buffer.
    pub fn flush(&mut self) {
        debug_assert!(
            self.inner.index <= 64,
            "too many bits written to RegisterBuffer"
        );
        self.writer.write_bits(self.inner.value, self.inner.index);
        self.inner = Default::default();
    }
}

/// A reader that can only hold 64 bits, but only uses registers instead of loads.
pub struct RegisterReader<'a, R: Read> {
    pub reader: &'a mut R,
    pub inner: RegisterBuffer,
}

// The purpose of this drop impl is to advance the reader if we encounter an error to check for EOF.
// Since all errors are equal when debug_assertions is off, we don't care if the error is EOF or not.
#[cfg(debug_assertions)]
impl<'a, R: Read> Drop for RegisterReader<'a, R> {
    fn drop(&mut self) {
        self.advance_reader();
    }
}

impl<'a, R: Read> RegisterReader<'a, R> {
    pub fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            inner: Default::default(),
        }
    }

    /// Only advances the reader. Doesn't refill the buffer.
    pub fn advance_reader(&mut self) {
        debug_assert!(
            self.inner.index <= 64,
            "too many bits read from RegisterBuffer"
        );
        self.reader.advance(self.inner.index);
        self.inner = Default::default();
    }

    /// Advances the reader and refills the buffer.
    pub fn refill(&mut self) -> Result<()> {
        self.advance_reader();
        self.inner.value = self.reader.peek_bits()?;
        self.inner.index = 0;
        Ok(())
    }
}

/// The inner part of [`RegisterWriter`] or [`RegisterReader`]. Allows recursive types to compile
/// because their reader's type doesn't depend on their input reader's type.
#[derive(Default)]
pub struct RegisterBuffer {
    value: Word,
    index: usize,
}

impl Write for RegisterBuffer {
    fn start_write(&mut self) {
        unimplemented!()
    }

    fn finish_write(&mut self) -> &[u8] {
        unimplemented!()
    }

    fn write_bit(&mut self, v: bool) {
        self.write_bits(v as Word, 1);
    }

    fn write_bits(&mut self, word: Word, bits: usize) {
        self.value |= word << self.index;
        self.index += bits;
    }

    fn write_bytes(&mut self, _: &[u8]) {
        unimplemented!()
    }
}

impl Read for RegisterBuffer {
    fn start_read(&mut self, _: &[u8]) {
        unimplemented!()
    }

    fn finish_read(&self) -> Result<()> {
        unimplemented!()
    }

    fn advance(&mut self, bits: usize) {
        self.index += bits;
    }

    fn peek_bits(&mut self) -> Result<Word> {
        debug_assert!(self.index < 64);
        let v = self.value >> self.index;
        Ok(v)
    }

    fn read_bit(&mut self) -> Result<bool> {
        Ok(self.read_bits(1)? != 0)
    }

    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        let v = self.peek_bits()? & (Word::MAX >> (WORD_BITS - bits));
        self.advance(bits);
        Ok(v)
    }

    fn read_bytes(&mut self, _: usize) -> Result<&[u8]> {
        unimplemented!()
    }

    fn reserve_bits(&self, _: usize) -> Result<()> {
        Ok(())
    }
}
