//! Client errors.

use std::io;

use thiserror::Error;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors from URL parsing, I/O, framing, or server RPC.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid `epochs://` URL.
    #[error("invalid epochs URL: {0}")]
    Url(String),

    /// TCP or framing I/O failure.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// JSON encode/decode failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Frame larger than the EPX max (16 MiB) or otherwise invalid.
    #[error("protocol framing error: {0}")]
    Frame(String),

    /// Server returned `ok: false`.
    #[error("EPX error: {0}")]
    Server(String),

    /// Unexpected / incomplete server payload.
    #[error("unexpected response: {0}")]
    Unexpected(String),

    /// Base64 decode of a CAS payload failed.
    #[error("base64 decode: {0}")]
    Base64(String),
}
