use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::Result;

/// A buffer that can only hold 64 bits, but only uses registers instead of load/store.
/// Can currently only read up to 63 bits at a time.
#[derive(Default)]
pub struct RegisterBuffer {
    value: Word,
    index: usize,
}

impl RegisterBuffer {
    pub fn write_to(&self, writer: &mut impl Write) {
        writer.write_bits(self.value, self.index)
    }

    pub fn peek_reader(reader: &mut impl Read) -> Result<Self> {
        Ok(Self {
            value: reader.peek_bits()?,
            index: 0,
        })
    }

    pub fn advance_reader(&self, reader: &mut impl Read) -> Result<()> {
        reader.advance(self.index)
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
        debug_assert!(bits != 64);
        self.value >>= bits;
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
        let v = self.value & (Word::MAX >> (WORD_BITS - bits));

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
