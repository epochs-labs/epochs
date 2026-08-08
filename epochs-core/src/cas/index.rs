//! `.epi` index format (44-byte fixed entries).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::codec::{ByteDecoder, ByteEncoder};
use crate::error::Result;
use crate::hash::Hash;

/// Size of a single index entry in bytes.
pub const INDEX_ENTRY_LEN: usize = 44;

/// Location of an object on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// Segment file id.
    pub segment_id: u16,
    /// Byte offset within the segment file.
    pub offset: u64,
    /// Total on-disk record length.
    pub record_len: u32,
}

/// Append-only hash index backed by `.epi` and an in-memory cache.
pub struct Index {
    file: File,
    entries: HashMap<Hash, IndexEntry>,
}

impl Index {
    /// Open or create an index at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;

        let mut entries = HashMap::new();
        file.seek(SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut offset = 0;
        while offset + INDEX_ENTRY_LEN <= buf.len() {
            let entry_bytes = &buf[offset..offset + INDEX_ENTRY_LEN];
            let entry = decode_entry(entry_bytes)?;
            entries.insert(entry.0, entry.1);
            offset += INDEX_ENTRY_LEN;
        }

        file.seek(SeekFrom::End(0))?;

        Ok(Self { file, entries })
    }

    /// Look up an entry by hash.
    pub fn get(&self, hash: &Hash) -> Option<IndexEntry> {
        self.entries.get(hash).copied()
    }

    /// Returns true if the hash is indexed.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.entries.contains_key(hash)
    }

    /// Append a new index entry (durable after [`Self::flush`]).
    pub fn insert(&mut self, hash: Hash, entry: IndexEntry) -> Result<()> {
        if self.entries.contains_key(&hash) {
            return Ok(());
        }

        let bytes = encode_entry(hash, entry);
        self.file.write_all(&bytes)?;
        self.entries.insert(hash, entry);
        Ok(())
    }

    /// fsync the index file (call at commit / durability boundaries).
    pub fn flush(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    /// Iterate all indexed content hashes.
    pub fn hashes(&self) -> impl Iterator<Item = Hash> + '_ {
        self.entries.keys().copied()
    }

    /// Find hashes whose hex representation starts with `prefix` (case-insensitive).
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<Hash> {
        let prefix = prefix.to_ascii_lowercase();
        self.entries
            .keys()
            .filter(|h| h.to_string().starts_with(&prefix))
            .copied()
            .collect()
    }
}

fn encode_entry(hash: Hash, entry: IndexEntry) -> [u8; INDEX_ENTRY_LEN] {
    let mut enc = ByteEncoder::new();
    enc.write_hash(&hash);
    enc.write_u16(entry.segment_id);
    enc.write_u48(entry.offset);
    enc.write_u32(entry.record_len);
    let mut out = [0u8; INDEX_ENTRY_LEN];
    out.copy_from_slice(&enc.buf);
    out
}

fn decode_entry(buf: &[u8]) -> Result<(Hash, IndexEntry)> {
    let mut dec = ByteDecoder::new(buf);
    let hash = dec.read_hash()?;
    let segment_id = dec.read_u16()?;
    let offset = dec.read_u48()?;
    let record_len = dec.read_u32()?;
    Ok((
        hash,
        IndexEntry {
            segment_id,
            offset,
            record_len,
        },
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::env;

    #[test]
    fn index_roundtrip() {
        let dir = env::temp_dir().join("epochs_index_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("index.epi");
        let mut idx = Index::open(&path).unwrap();
        let hash = Hash::of_bytes(b"test");
        let entry = IndexEntry {
            segment_id: 0,
            offset: 16,
            record_len: 100,
        };
        idx.insert(hash, entry).unwrap();
        idx.flush().unwrap();

        drop(idx);
        let idx2 = Index::open(&path).unwrap();
        assert_eq!(idx2.get(&hash), Some(entry));

        std::fs::remove_dir_all(&dir).ok();
    }
}
