use crate::buffer::WithCapacity;
use crate::read::Read;
use crate::word::*;
use crate::write::Write;
use crate::{Result, E};
use bitvec::domain::Domain;
use bitvec::prelude::*;

/// A slow proof of concept [`Buffer`] that uses [`BitVec`]. Useful for comparison.
#[derive(Debug, Default)]
pub struct BitBuffer {
    bits: BitVec<u8, Lsb0>,
    read: usize,
    tmp: Box<[u8]>,
}

impl WithCapacity for BitBuffer {
    fn capacity(&self) -> usize {
        self.bits.capacity() / u8::BITS as usize
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            bits: BitVec::with_capacity(cap * u8::BITS as usize),
            ..Default::default()
        }
    }
}

impl Write for BitBuffer {
    fn start_write(&mut self) {
        self.bits.clear();
        self.read = 0;
    }

    fn finish_write(&mut self) -> &[u8] {
        self.bits.force_align();
        self.bits.as_raw_slice()
    }

    fn write_bit(&mut self, v: bool) {
        self.bits.push(v);
    }

    fn write_bits(&mut self, word: Word, bits: usize) {
        self.bits
            .extend_from_bitslice(&BitSlice::<u8, Lsb0>::from_slice(&word.to_le_bytes())[..bits]);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.bits
            .extend_from_bitslice(&BitSlice::<u8, Lsb0>::from_slice(bytes));
    }
}

impl BitBuffer {
    fn read_slice(&mut self, bits: usize) -> Result<&BitSlice<u8, Lsb0>> {
        let slice = self.bits[self.read..]
            .get(..bits)
            .ok_or_else(|| E::Eof.e())?;
        self.read += bits;
        Ok(slice)
    }
}

impl Read for BitBuffer {
    fn start_read(&mut self, bytes: &[u8]) {
        self.bits.clear();
        self.bits.extend_from_raw_slice(bytes);
        self.read = 0;
    }

    fn finish_read(&self) -> Result<()> {
        // Can't use remaining because of borrow checker.
        let remaining = &self.bits[self.read..];
        if remaining.is_empty() {
            return Ok(());
        }

        // Make sure no trailing 1 bits or zero bytes.
        let e = match remaining.domain() {
            Domain::Enclave(e) => e,
            Domain::Region { head, body, tail } => {
                if !body.is_empty() {
                    return Err(E::ExpectedEof.e());
                }
                head.xor(tail).ok_or_else(|| E::ExpectedEof.e())?
            }
        };
        (e.into_bitslice().count_ones() == 0)
            .then_some(())
            .ok_or_else(|| E::ExpectedEof.e())
    }

    fn advance(&mut self, bits: usize) -> Result<()> {
        self.read_slice(bits)?;
        Ok(())
    }

    fn peek_bits(&mut self) -> Result<Word> {
        let slice = &self.bits[self.read..];
        let bits = slice.len().min(64);

        let mut v = [0; 8];
        BitSlice::<u8, Lsb0>::from_slice_mut(&mut v)[..bits].copy_from_bitslice(&slice[..bits]);
        Ok(Word::from_le_bytes(v))
    }

    fn read_bit(&mut self) -> Result<bool> {
        Ok(self.read_slice(1)?[0])
    }

    fn read_bits(&mut self, bits: usize) -> Result<Word> {
        let slice = self.read_slice(bits)?;

        let mut v = [0; 8];
        BitSlice::<u8, Lsb0>::from_slice_mut(&mut v)[..bits].copy_from_bitslice(slice);
        Ok(Word::from_le_bytes(v))
    }

    fn read_bytes(&mut self, len: usize) -> Result<&[u8]> {
        // Take to avoid borrowing issue.
        let mut tmp = std::mem::take(&mut self.tmp);

        let bits = len
            .checked_mul(u8::BITS as usize)
            .ok_or_else(|| E::Eof.e())?;
        let slice = self.read_slice(bits)?;

        // Only allocate after reserve_read to prevent memory exhaustion attacks.
        if tmp.len() < len {
            tmp = vec![0; len.next_power_of_two()].into_boxed_slice()
        }

        tmp.as_mut_bits()[..slice.len()].copy_from_bitslice(slice);
        self.tmp = tmp;
        Ok(&self.tmp[..len])
    }

    fn reserve_bits(&self, bits: usize) -> Result<()> {
        if bits <= self.bits[self.read..].len() {
            Ok(())
        } else {
            Err(E::Eof.e())
        }
    }
}
