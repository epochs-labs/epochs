//! EpochQL execution engine.
//!
//! Runs parsed statements against [`epochs_core::DiskStore`]. When a schema is
//! present (via `.eql` migrations), commits update [`Commit::index_roots`].

mod engine;
mod value;

pub use engine::{Engine, ExecError};
pub use value::{ExecResult, MutationResult, QueryResult, Value};
