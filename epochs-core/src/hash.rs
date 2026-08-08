//! Content-addressable hash type using BLAKE3.

use crate::error::{EpochsError, Result};
use std::fmt;
use std::str::FromStr;

/// A 256-bit BLAKE3 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    /// The all-zero hash, useful for testing.
    pub const ZERO: Self = Self([0u8; 32]);

    /// Construct from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Compute a content hash from raw bytes.
    ///
    /// Large payloads (≥16 KiB) use BLAKE3's parallel hasher when the
    /// `parallel-hash` feature is enabled.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        #[cfg(feature = "parallel-hash")]
        {
            const PARALLEL_THRESHOLD: usize = 16 * 1024;
            if bytes.len() >= PARALLEL_THRESHOLD {
                let mut hasher = blake3::Hasher::new();
                hasher.update_rayon(bytes);
                return Self(*hasher.finalize().as_bytes());
            }
        }
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Parse a hash from a 64-character lowercase hexadecimal string.
    pub fn from_hex(s: &str) -> Result<Self> {
        if s.len() != 64 {
            return Err(EpochsError::InvalidTarget(format!(
                "hash must be 64 hex characters, got {}",
                s.len()
            )));
        }

        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex_pair = std::str::from_utf8(chunk)
                .map_err(|e| EpochsError::InvalidTarget(format!("invalid hex encoding: {e}")))?;
            bytes[i] = u8::from_str_radix(hex_pair, 16)
                .map_err(|e| EpochsError::InvalidTarget(format!("invalid hex digit: {e}")))?;
        }

        Ok(Self(bytes))
    }

    /// Return the raw hash bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({self})")
    }
}

impl FromStr for Hash {
    type Err = EpochsError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_hex(s)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn hash_roundtrip_hex() {
        let hash = Hash::of_bytes(b"hello");
        let hex = hash.to_string();
        assert_eq!(hex.len(), 64);
        assert_eq!(Hash::from_hex(&hex).unwrap(), hash);
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(Hash::of_bytes(b"test"), Hash::of_bytes(b"test"));
    }
}
