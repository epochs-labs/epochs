//! Error types for the epochs core library.

use std::fmt;
use std::io;

use crate::hash::Hash;

/// Result type alias for epochs core operations.
pub type Result<T> = std::result::Result<T, EpochsError>;

/// Errors that can occur during epochs operations.
#[derive(Debug, PartialEq, Eq)]
pub enum EpochsError {
    /// A commit with the given hash was not found.
    CommitNotFound(Hash),
    /// A branch with the given name was not found.
    BranchNotFound(String),
    /// A branch with the given name already exists.
    BranchExists(String),
    /// The checkout target is neither a valid branch name nor a commit hash.
    InvalidTarget(String),
    /// A storage operation failed.
    Storage(StorageError),
    /// Encoding or decoding failed.
    Codec(String),
    /// The commit is invalid (e.g. missing parent).
    InvalidCommit(String),
    /// I/O error.
    Io(String),
}

/// Errors from the content-addressable storage layer.
#[derive(Debug, PartialEq, Eq)]
pub enum StorageError {
    /// Content hash does not match the stored bytes.
    HashMismatch {
        /// Expected content hash.
        expected: Hash,
        /// Hash computed from the provided bytes.
        computed: Hash,
    },
    /// An object with the given hash was not found.
    NotFound(Hash),
    /// Record failed CRC verification.
    CrcMismatch,
    /// Corrupted or invalid on-disk record.
    Corrupt(String),
}

impl fmt::Display for EpochsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommitNotFound(h) => write!(f, "commit not found: {h}"),
            Self::BranchNotFound(name) => write!(f, "branch not found: {name}"),
            Self::BranchExists(name) => write!(f, "branch already exists: {name}"),
            Self::InvalidTarget(t) => write!(f, "invalid checkout target: {t}"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
            Self::Codec(msg) => write!(f, "codec error: {msg}"),
            Self::InvalidCommit(msg) => write!(f, "invalid commit: {msg}"),
            Self::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashMismatch { expected, computed } => {
                write!(f, "hash mismatch: expected {expected}, computed {computed}")
            }
            Self::NotFound(h) => write!(f, "object not found: {h}"),
            Self::CrcMismatch => write!(f, "crc mismatch"),
            Self::Corrupt(msg) => write!(f, "corrupt record: {msg}"),
        }
    }
}

impl std::error::Error for EpochsError {}
impl std::error::Error for StorageError {}

impl From<io::Error> for EpochsError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<StorageError> for EpochsError {
    fn from(err: StorageError) -> Self {
        Self::Storage(err)
    }
}
