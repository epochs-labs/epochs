//! Content-addressable append-only storage.

mod index;
mod mem;
mod record;
mod store;

use std::sync::Arc;

use crate::error::Result;
use crate::hash::Hash;

pub use mem::MemCas;
pub use record::{Record, RecordType};
pub use store::CasStore;

/// CAS backend used by the persistent HAMT and commit layer.
pub trait CasBackend {
    /// Store payload, returning content hash.
    fn put(&mut self, record_type: RecordType, payload: &[u8]) -> Result<Hash>;

    /// Retrieve record type and payload by hash (shared buffer, zero-copy on cache hit).
    fn get_record(&mut self, hash: &Hash) -> Result<(RecordType, Arc<[u8]>)>;
}
