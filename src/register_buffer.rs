use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::Result;

/// A buffer that can only hold 64 bits, but only uses registers instead of load/store.
#[derive(Default)]
pub struct RegisterBuffer {
    value: Word,
    index: usize,
}

impl RegisterBuffer {
    /// Writes the contents of the buffer to `writer` and clears the buffer.
    pub fn flush(&mut self, writer: &mut impl Write) {
        debug_assert!(self.index <= 64, "too many bits written to RegisterBuffer");
        writer.write_bits(self.value, self.index);
        *self = Self::default();
    }

    /// Advances by the amount read from the buffer and refills the buffer.
    pub fn refill(&mut self, reader: &mut impl Read) -> Result<()> {
        self.advance_reader(reader)?;
        self.value = reader.peek_bits()?;
        self.index = 0;
        Ok(())
    }

    /// Only advances the reader. Doesn't refill the buffer.
    pub fn advance_reader(&mut self, reader: &mut impl Read) -> Result<()> {
        debug_assert!(self.index <= 64, "too many bits read from RegisterBuffer");
        reader.advance(self.index)?;
        *self = Self::default();
        Ok(())
    }
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

    fn advance(&mut self, bits: usize) -> Result<()> {
        self.index += bits;
        Ok(())
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

        self.advance(bits)?;
        Ok(v)
    }

    fn read_bytes(&mut self, _: usize) -> Result<&[u8]> {
        unimplemented!()
    }

    fn reserve_bits(&self, _: usize) -> Result<()> {
        Ok(())
    }
}
