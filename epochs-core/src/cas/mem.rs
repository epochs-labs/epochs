//! In-memory CAS for unit tests.

use std::collections::HashMap;
use std::sync::Arc;

use crate::cas::{CasBackend, RecordType};
use crate::error::{EpochsError, Result, StorageError};
use crate::hash::Hash;

/// In-memory content-addressable store for tests.
#[derive(Debug, Default, Clone)]
pub struct MemCas {
    objects: HashMap<Hash, (RecordType, Arc<[u8]>)>,
}

impl MemCas {
    /// Create an empty in-memory CAS.
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
        }
    }

    /// Returns true if hash exists.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.objects.contains_key(hash)
    }
}

impl CasBackend for MemCas {
    fn put(&mut self, record_type: RecordType, payload: &[u8]) -> Result<Hash> {
        let hash = Hash::of_bytes(payload);
        self.objects
            .entry(hash)
            .or_insert_with(|| (record_type, Arc::<[u8]>::from(payload)));
        Ok(hash)
    }

    fn get_record(&mut self, hash: &Hash) -> Result<(RecordType, Arc<[u8]>)> {
        self.objects
            .get(hash)
            .map(|(ty, payload)| (*ty, Arc::clone(payload)))
            .ok_or(EpochsError::Storage(StorageError::NotFound(*hash)))
    }
}
