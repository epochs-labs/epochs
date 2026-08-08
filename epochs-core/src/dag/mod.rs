//! Generic DAG graph algorithms for version-controlled stores.

mod algorithms;

pub use algorithms::{ancestors_within_depth, collect_ancestors, is_ancestor, merge_base};
