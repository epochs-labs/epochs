//! Binary decoder for little-endian formats.

use crate::error::{EpochsError, Result};
use crate::hash::Hash;

/// Cursor over a byte slice for decoding.
pub struct ByteDecoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteDecoder<'a> {
    /// Create a decoder over `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Current cursor position.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Remaining unread bytes.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Read a single byte.
    pub fn read_u8(&mut self) -> Result<u8> {
        let slice = self.read_slice(1)?;
        Ok(slice[0])
    }

    /// Read a little-endian u16.
    pub fn read_u16(&mut self) -> Result<u16> {
        let slice = self.read_slice(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    /// Read a little-endian u32.
    pub fn read_u32(&mut self) -> Result<u32> {
        let slice = self.read_slice(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    /// Read a little-endian u48 from 6 bytes.
    pub fn read_u48(&mut self) -> Result<u64> {
        let slice = self.read_slice(6)?;
        let mut bytes = [0u8; 8];
        bytes[..6].copy_from_slice(slice);
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a little-endian u64.
    pub fn read_u64(&mut self) -> Result<u64> {
        let slice = self.read_slice(8)?;
        Ok(u64::from_le_bytes([
            slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
        ]))
    }

    /// Read a 32-byte content hash.
    pub fn read_hash(&mut self) -> Result<Hash> {
        let slice = self.read_slice(32)?;
        let mut h = [0u8; 32];
        h.copy_from_slice(slice);
        Ok(Hash::from_bytes(h))
    }

    /// Read exactly `len` bytes.
    pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.pos + len > self.buf.len() {
            return Err(EpochsError::Codec(format!(
                "unexpected EOF: need {len} bytes, have {}",
                self.buf.len() - self.pos
            )));
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// Read the remaining bytes.
    pub fn read_rest(&mut self) -> Result<&'a [u8]> {
        let slice = &self.buf[self.pos..];
        self.pos = self.buf.len();
        Ok(slice)
    }
}
