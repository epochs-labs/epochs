//! # epochs-core
//!
//! Version-controlled Merkle-DAG engine built on std I/O, BLAKE3, persistent HAMT,
//! and an append-only content-addressable store (`.epl` / `.epi`).

#![warn(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod branch;
pub mod cache;
pub mod cas;
pub mod codec;
pub mod commit;
pub mod crc32;
pub mod dag;
pub mod error;
pub mod hamt;
pub mod hash;
pub mod repo;
pub mod store;

pub use branch::Branch;
pub use cas::{CasBackend, CasStore, MemCas, RecordType};
pub use codec::{ByteDecoder, ByteEncoder};
pub use commit::Commit;
pub use dag::{ancestors_within_depth, collect_ancestors, is_ancestor, merge_base};
pub use error::{EpochsError, Result, StorageError};
pub use hamt::{HamtNode, PersistentHamt};
pub use hash::Hash;
pub use repo::Repo;
pub use store::{DagStore, DiskStore, HamtOp};
