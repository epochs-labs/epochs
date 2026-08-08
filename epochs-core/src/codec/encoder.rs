//! Binary encoder for little-endian formats.

use crate::hash::Hash;

/// Append-only byte buffer encoder.
#[derive(Debug, Default, Clone)]
pub struct ByteEncoder {
    /// Encoded bytes.
    pub buf: Vec<u8>,
}

impl ByteEncoder {
    /// Create an empty encoder.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Create an encoder with a reserved capacity hint.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
        }
    }

    /// Clear the buffer for reuse (keeps capacity).
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Write a single byte.
    pub fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// Write a little-endian u16.
    pub fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a little-endian u32.
    pub fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a little-endian u48 as 6 bytes.
    pub fn write_u48(&mut self, v: u64) {
        let bytes = v.to_le_bytes();
        self.buf.extend_from_slice(&bytes[..6]);
    }

    /// Write a little-endian u64.
    pub fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a 32-byte content hash.
    pub fn write_hash(&mut self, hash: &Hash) {
        self.buf.extend_from_slice(hash.as_bytes());
    }

    /// Write raw bytes.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }
}
