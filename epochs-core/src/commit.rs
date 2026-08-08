//! Commit payload codec (CAS record type 1 body).
//!
//! # Versions
//!
//! - **v1** — parents, root_hamt, timestamp, message
//! - **v2** — v1 fields plus `index_roots` (secondary index HAMT roots)
//!
//! New commits are always written as v2. v1 payloads still decode (empty index map).

use std::collections::BTreeMap;

use crate::codec::ByteDecoder;
use crate::error::{EpochsError, Result};
use crate::hash::Hash;

/// Current commit payload format version (writes).
pub const COMMIT_VERSION: u8 = 2;

/// Legacy commit payload version (reads still supported).
pub const COMMIT_VERSION_V1: u8 = 1;

/// A commit node in the version-controlled DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// Parent commit hashes (empty for genesis commits).
    pub parents: Vec<Hash>,
    /// HAMT root hash for this snapshot (`Hash::ZERO` if empty).
    pub root_hamt: Hash,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
    /// Commit message.
    pub message: String,
    /// Secondary index roots: `"collection.by_path"` → index HAMT root hash.
    ///
    /// Empty until schema / path indexing is enabled. Stubbed in the on-disk
    /// format now so later index support does not break commit hashing layout.
    pub index_roots: BTreeMap<String, Hash>,
}

impl Commit {
    /// Create a commit with explicit secondary index roots.
    pub fn with_indexes(
        parents: Vec<Hash>,
        root_hamt: Hash,
        timestamp: u64,
        message: impl Into<String>,
        index_roots: BTreeMap<String, Hash>,
    ) -> Self {
        Self {
            parents,
            root_hamt,
            timestamp,
            message: message.into(),
            index_roots,
        }
    }

    /// Create a commit with no index roots.
    pub fn new(
        parents: Vec<Hash>,
        root_hamt: Hash,
        timestamp: u64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            parents,
            root_hamt,
            timestamp,
            message: message.into(),
            index_roots: BTreeMap::new(),
        }
    }

    /// Encode canonical commit payload bytes (stored in CAS) as v2.
    pub fn encode_payload(&self) -> Vec<u8> {
        crate::codec::encode_with(|enc| {
            enc.write_u8(COMMIT_VERSION);
            enc.write_u8(self.parents.len() as u8);
            for parent in &self.parents {
                enc.write_hash(parent);
            }
            enc.write_hash(&self.root_hamt);
            enc.write_u64(self.timestamp);
            let msg = self.message.as_bytes();
            enc.write_u16(msg.len() as u16);
            enc.write_bytes(msg);

            enc.write_u16(self.index_roots.len() as u16);
            for (name, hash) in &self.index_roots {
                let bytes = name.as_bytes();
                enc.write_u16(bytes.len() as u16);
                enc.write_bytes(bytes);
                enc.write_hash(hash);
            }
        })
    }

    /// Decode commit payload bytes (v1 or v2).
    pub fn decode_payload(buf: &[u8]) -> Result<Self> {
        let mut dec = ByteDecoder::new(buf);
        let version = dec.read_u8()?;
        if version != COMMIT_VERSION && version != COMMIT_VERSION_V1 {
            return Err(EpochsError::Codec(format!(
                "unsupported commit version: {version}"
            )));
        }

        let parent_count = dec.read_u8()? as usize;
        let mut parents = Vec::with_capacity(parent_count);
        for _ in 0..parent_count {
            parents.push(dec.read_hash()?);
        }

        let root_hamt = dec.read_hash()?;
        let timestamp = dec.read_u64()?;
        let msg_len = dec.read_u16()? as usize;
        let msg_bytes = dec.read_slice(msg_len)?;
        let message = String::from_utf8(msg_bytes.to_vec())
            .map_err(|e| EpochsError::Codec(format!("commit message not valid utf-8: {e}")))?;

        let mut index_roots = BTreeMap::new();
        if version >= COMMIT_VERSION {
            let count = dec.read_u16()? as usize;
            for _ in 0..count {
                let name_len = dec.read_u16()? as usize;
                let name_bytes = dec.read_slice(name_len)?;
                let name = String::from_utf8(name_bytes.to_vec()).map_err(|e| {
                    EpochsError::Codec(format!("index root name not valid utf-8: {e}"))
                })?;
                let hash = dec.read_hash()?;
                index_roots.insert(name, hash);
            }
        }

        Ok(Self {
            parents,
            root_hamt,
            timestamp,
            message,
            index_roots,
        })
    }

    /// Content-addressed commit hash.
    pub fn id(&self) -> Hash {
        Hash::of_bytes(&self.encode_payload())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::codec::ByteEncoder;

    #[test]
    fn v2_roundtrip_empty_indexes() {
        let c = Commit::new(vec![], Hash::ZERO, 1, "hi");
        let decoded = Commit::decode_payload(&c.encode_payload()).unwrap();
        assert_eq!(decoded, c);
        assert!(decoded.index_roots.is_empty());
    }

    #[test]
    fn v2_roundtrip_with_indexes() {
        let mut c = Commit::new(vec![], Hash::of_bytes(b"root"), 2, "idx");
        c.index_roots
            .insert("items.by_id".into(), Hash::of_bytes(b"idx1"));
        let decoded = Commit::decode_payload(&c.encode_payload()).unwrap();
        assert_eq!(decoded.index_roots.len(), 1);
        assert_eq!(
            decoded.index_roots.get("items.by_id"),
            Some(&Hash::of_bytes(b"idx1"))
        );
    }

    #[test]
    fn v1_payload_still_decodes() {
        // Manually craft a v1 payload
        let mut enc = ByteEncoder::new();
        enc.write_u8(COMMIT_VERSION_V1);
        enc.write_u8(0);
        enc.write_hash(&Hash::ZERO);
        enc.write_u64(99);
        enc.write_u16(3);
        enc.write_bytes(b"old");
        let c = Commit::decode_payload(&enc.buf).unwrap();
        assert_eq!(c.message, "old");
        assert!(c.index_roots.is_empty());
    }
}
