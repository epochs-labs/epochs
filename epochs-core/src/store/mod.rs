//! DAG store implementations.

mod dag;
mod disk;

pub use dag::{DagStore, HamtOp};
pub use disk::DiskStore;
