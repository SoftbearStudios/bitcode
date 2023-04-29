use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::Result;

/// A writer that can only hold 64 bits, but only uses registers instead of load/store.
pub struct RegisterWriter<'a, W: Write> {
    pub writer: &'a mut W,
    value: Word,
    index: usize,
}

impl<'a, W: Write> RegisterWriter<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            value: 0,
            index: 0,
        }
    }
}

impl<'a, W: Write> RegisterWriter<'a, W> {
    /// Writes the contents of the buffer to `writer` and clears the buffer.
    pub fn flush(&mut self) {
        debug_assert!(self.index <= 64, "too many bits written to RegisterBuffer");
        self.writer.write_bits(self.value, self.index);
        self.value = 0;
        self.index = 0;
    }
}

impl<'a, W: Write> Write for RegisterWriter<'a, W> {
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

/// A reader that can only hold 64 bits, but only uses registers instead of loads.
pub struct RegisterReader<'a, R: Read> {
    pub reader: &'a mut R,
    value: Word,
    index: usize,
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
            value: 0,
            index: 0,
        }
    }

    /// Only advances the reader. Doesn't refill the buffer.
    pub fn advance_reader(&mut self) {
        debug_assert!(self.index <= 64, "too many bits read from RegisterBuffer");
        self.reader.advance(self.index);
        self.value = 0;
        self.index = 0;
    }

    /// Advances the reader and refills the buffer.
    pub fn refill(&mut self) -> Result<()> {
        self.advance_reader();
        self.value = self.reader.peek_bits()?;
        self.index = 0;
        Ok(())
    }
}

impl<'a, R: Read> Read for RegisterReader<'a, R> {
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
        Ok(self.value)
    }

    fn read_bit(&mut self) -> Result<bool> {
        Ok(self.read_bits(1)? != 0)
    }

    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        debug_assert!(self.index < 64);
        let v = (self.value >> self.index) & (Word::MAX >> (WORD_BITS - bits));

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
