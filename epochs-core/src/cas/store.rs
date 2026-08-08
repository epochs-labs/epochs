//! Segmented append-only CAS on disk.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::LruMap;
use crate::cas::index::{Index, IndexEntry};
use crate::cas::record::{
    decode_record, encode_record, encode_segment_header, Record, SEGMENT_HEADER_LEN, SEGMENT_MAGIC,
};
use crate::cas::RecordType;
use crate::error::{EpochsError, Result, StorageError};
use crate::hash::Hash;

#[cfg(feature = "mmap")]
use std::collections::HashMap;

/// Maximum segment size before rotation (64 MiB).
pub const SEGMENT_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Default bound on in-memory object cache (immutable CAS entries only).
const WRITE_CACHE_CAP: usize = 8_192;

/// Default max sealed segments kept mmapped (file-backed; still costs RSS when faulted).
#[cfg(feature = "mmap")]
const MMAP_SEGMENTS_CAP: usize = 4;

/// Write buffer for the active segment (keep modest — counts toward RSS).
const SEGMENT_BUF_BYTES: usize = 64 * 1024;

/// Cached CAS payload (shared across HAMT loads).
type CachedPayload = (RecordType, Arc<[u8]>);

/// Disk-backed content-addressable store using `.epl` logs and `.epi` index.
pub struct CasStore {
    repo_path: PathBuf,
    index: Index,
    current_segment_id: u16,
    current_file: BufWriter<File>,
    current_offset: u64,
    /// True when segment/index bytes may not yet be fsync'd.
    dirty: bool,
    /// Recently written/read payloads (zero-copy on hit via [`Arc`]).
    write_cache: LruMap<Hash, CachedPayload>,
    /// Memory-maps of sealed (non-current) segments, when `mmap` feature is on.
    #[cfg(feature = "mmap")]
    segment_maps: HashMap<u16, memmap2::Mmap>,
    /// Max sealed segments to keep mapped (0 = disable mmap).
    #[cfg(feature = "mmap")]
    mmap_cap: usize,
}

impl CasStore {
    /// Open a CAS store within a repository directory.
    pub fn open(repo_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(repo_path.join("data"))?;
        let index = Index::open(&repo_path.join("index.epi"))?;

        let segment_id = find_latest_segment(repo_path).unwrap_or(0);
        let segment_path = segment_path_for(repo_path, segment_id);
        let (file, offset) = open_or_create_segment(&segment_path, segment_id)?;

        Ok(Self {
            repo_path: repo_path.to_path_buf(),
            index,
            current_segment_id: segment_id,
            current_file: BufWriter::with_capacity(SEGMENT_BUF_BYTES, file),
            current_offset: offset,
            dirty: false,
            write_cache: LruMap::new(WRITE_CACHE_CAP),
            #[cfg(feature = "mmap")]
            segment_maps: HashMap::new(),
            #[cfg(feature = "mmap")]
            mmap_cap: MMAP_SEGMENTS_CAP,
        })
    }

    /// Bound the CAS object LRU (`cap` entries of immutable payloads).
    pub fn set_object_cache_cap(&mut self, cap: usize) {
        self.write_cache = LruMap::new(cap);
    }

    /// Max sealed segments to keep memory-mapped (`0` disables mmap).
    #[cfg(feature = "mmap")]
    pub fn set_mmap_cap(&mut self, cap: usize) {
        self.mmap_cap = cap;
        while self.segment_maps.len() > self.mmap_cap {
            if let Some(&oldest) = self.segment_maps.keys().min() {
                self.segment_maps.remove(&oldest);
            } else {
                break;
            }
        }
    }

    #[cfg(not(feature = "mmap"))]
    pub fn set_mmap_cap(&mut self, _cap: usize) {}

    /// Returns true if the hash is stored.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.index.contains(hash)
    }

    /// Store payload, returning its content hash. Deduplicates identical payloads.
    ///
    /// Writes are buffered; call [`Self::flush`] (or drop the store) for durability.
    pub fn put(&mut self, record_type: RecordType, payload: &[u8]) -> Result<Hash> {
        let hash = Hash::of_bytes(payload);
        if self.index.contains(&hash) {
            return Ok(hash);
        }

        self.ensure_segment_capacity(41 + payload.len() as u64 + 4)?;

        let record_bytes = encode_record(record_type, payload, hash);
        let record_len = record_bytes.len() as u32;
        let offset = self.current_offset;

        self.current_file.write_all(&record_bytes)?;

        self.index.insert(
            hash,
            IndexEntry {
                segment_id: self.current_segment_id,
                offset,
                record_len,
            },
        )?;

        self.current_offset += record_len as u64;
        self.dirty = true;
        let arc: Arc<[u8]> = Arc::from(payload);
        self.write_cache.insert(hash, (record_type, arc));
        Ok(hash)
    }

    /// Persist buffered CAS + index writes to stable storage.
    pub fn flush(&mut self) -> Result<()> {
        self.current_file.flush()?;
        if !self.dirty {
            return Ok(());
        }
        self.current_file.get_ref().sync_data()?;
        self.index.flush()?;
        self.dirty = false;
        // Active segment grew — drop stale mmap if any.
        #[cfg(feature = "mmap")]
        {
            self.segment_maps.remove(&self.current_segment_id);
        }
        Ok(())
    }

    /// Shared payload load (cache-friendly). Prefer this for HAMT / hot paths.
    pub fn get_payload(&mut self, hash: &Hash) -> Result<(RecordType, Arc<[u8]>)> {
        if let Some((ty, payload)) = self.write_cache.get(hash) {
            return Ok((*ty, Arc::clone(payload)));
        }

        let entry = self
            .index
            .get(hash)
            .ok_or(EpochsError::Storage(StorageError::NotFound(*hash)))?;

        let record_buf = self.read_record_bytes(entry)?;
        let rec = decode_record(&record_buf)?;
        let arc: Arc<[u8]> = Arc::from(rec.payload.as_slice());
        self.write_cache
            .insert(*hash, (rec.record_type, Arc::clone(&arc)));
        Ok((rec.record_type, arc))
    }

    /// Retrieve and verify a full record by content hash (owned payload copy).
    pub fn get_record(&mut self, hash: &Hash) -> Result<Record> {
        let (record_type, payload) = self.get_payload(hash)?;
        Ok(Record {
            record_type,
            content_hash: *hash,
            payload: payload.to_vec(),
        })
    }

    /// Retrieve and verify payload by content hash.
    pub fn get(&mut self, hash: &Hash) -> Result<Vec<u8>> {
        Ok(self.get_payload(hash)?.1.to_vec())
    }

    /// Find content hashes whose hex form starts with `prefix`.
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<Hash> {
        self.index.find_by_prefix(prefix)
    }

    /// All indexed content hashes.
    pub fn hashes(&self) -> Vec<Hash> {
        self.index.hashes().collect()
    }

    /// Repository root path.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    fn read_record_bytes(&mut self, entry: IndexEntry) -> Result<Vec<u8>> {
        // Always flush the active writer so the on-disk view is complete.
        if entry.segment_id == self.current_segment_id {
            self.current_file.flush()?;
        }

        #[cfg(feature = "mmap")]
        {
            // Prefer mmap for sealed segments when enabled.
            if self.mmap_cap > 0 && entry.segment_id != self.current_segment_id {
                if let Ok(map) = self.ensure_mmap(entry.segment_id) {
                    let start = entry.offset as usize;
                    let end = start + entry.record_len as usize;
                    if end <= map.len() {
                        return Ok(map[start..end].to_vec());
                    }
                }
            }
        }

        let path = segment_path_for(&self.repo_path, entry.segment_id);
        let mut file = File::open(&path)?;
        file.seek(SeekFrom::Start(entry.offset))?;
        let mut record_buf = vec![0u8; entry.record_len as usize];
        file.read_exact(&mut record_buf)?;
        Ok(record_buf)
    }

    #[cfg(feature = "mmap")]
    fn ensure_mmap(&mut self, segment_id: u16) -> Result<&memmap2::Mmap> {
        if self.mmap_cap == 0 {
            return Err(EpochsError::Storage(StorageError::Corrupt(
                "mmap disabled".into(),
            )));
        }
        if !self.segment_maps.contains_key(&segment_id) {
            let path = segment_path_for(&self.repo_path, segment_id);
            let file = File::open(&path)?;
            // SAFETY: segment files are append-only; we only mmap sealed segments.
            let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| {
                EpochsError::Storage(StorageError::Corrupt(format!("mmap failed: {e}")))
            })?;
            while self.segment_maps.len() >= self.mmap_cap {
                if let Some(&oldest) = self.segment_maps.keys().min() {
                    self.segment_maps.remove(&oldest);
                } else {
                    break;
                }
            }
            self.segment_maps.insert(segment_id, map);
        }
        self.segment_maps.get(&segment_id).ok_or_else(|| {
            EpochsError::Storage(StorageError::Corrupt("mmap missing after insert".into()))
        })
    }

    fn ensure_segment_capacity(&mut self, needed: u64) -> Result<()> {
        if self.current_offset + needed > SEGMENT_MAX_BYTES {
            self.rotate_segment()?;
        }
        Ok(())
    }

    fn rotate_segment(&mut self) -> Result<()> {
        self.flush()?;
        // Do not eagerly mmap on rotate — maps are created lazily and capped.

        self.current_segment_id = self.current_segment_id.checked_add(1).ok_or_else(|| {
            EpochsError::Storage(StorageError::Corrupt("segment id overflow".into()))
        })?;

        let path = segment_path_for(&self.repo_path, self.current_segment_id);
        let (file, offset) = open_or_create_segment(&path, self.current_segment_id)?;
        self.current_file = BufWriter::with_capacity(SEGMENT_BUF_BYTES, file);
        self.current_offset = offset;
        Ok(())
    }
}

impl Drop for CasStore {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn segment_path_for(repo_path: &Path, segment_id: u16) -> PathBuf {
    repo_path.join("data").join(format!("{segment_id:06}.epl"))
}

fn find_latest_segment(repo_path: &Path) -> Option<u16> {
    let data_dir = repo_path.join("data");
    let mut max_id = None;
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".epl") {
                if let Ok(id) = stem.parse::<u16>() {
                    max_id = Some(max_id.map_or(id, |m: u16| m.max(id)));
                }
            }
        }
    }
    max_id
}

fn open_or_create_segment(path: &Path, segment_id: u16) -> Result<(File, u64)> {
    let exists = path.exists();
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)?;

    if !exists {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let header = encode_segment_header(segment_id as u32, created_at);
        file.write_all(&header)?;
        file.sync_data()?;
        Ok((file, SEGMENT_HEADER_LEN))
    } else {
        let len = file.metadata()?.len();
        if len < SEGMENT_HEADER_LEN {
            return Err(EpochsError::Storage(StorageError::Corrupt(
                "segment shorter than header".into(),
            )));
        }
        let mut magic = [0u8; 4];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut magic)?;
        if magic != SEGMENT_MAGIC {
            return Err(EpochsError::Storage(StorageError::Corrupt(
                "invalid segment magic".into(),
            )));
        }
        Ok((file, len))
    }
}

impl crate::cas::CasBackend for CasStore {
    fn put(&mut self, record_type: RecordType, payload: &[u8]) -> Result<Hash> {
        CasStore::put(self, record_type, payload)
    }

    fn get_record(&mut self, hash: &Hash) -> Result<(RecordType, Arc<[u8]>)> {
        CasStore::get_payload(self, hash)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::env;

    fn temp_repo(name: &str) -> PathBuf {
        env::temp_dir().join(name)
    }

    #[test]
    fn cas_roundtrip_and_dedup() {
        let dir = temp_repo("epochs_cas_test_opt");
        let _ = std::fs::remove_dir_all(&dir);

        let mut cas = CasStore::open(&dir).unwrap();
        let payload = b"hello epochs".to_vec();
        let h1 = cas.put(RecordType::HamtLeaf, &payload).unwrap();
        let h2 = cas.put(RecordType::HamtLeaf, &payload).unwrap();
        assert_eq!(h1, h2);
        assert!(cas.contains(&h1));

        let got = cas.get(&h1).unwrap();
        assert_eq!(got, payload);
        cas.flush().unwrap();

        drop(cas);
        let mut cas2 = CasStore::open(&dir).unwrap();
        assert_eq!(cas2.get(&h1).unwrap(), payload);

        std::fs::remove_dir_all(&dir).ok();
    }
}
