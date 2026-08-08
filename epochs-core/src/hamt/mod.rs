//! Persistent HAMT (32-way bitmap trie).

mod node;
mod tree;

pub use node::HamtNode;
pub use tree::{DiffOp, PersistentHamt};
