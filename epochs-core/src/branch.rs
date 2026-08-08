//! Branch reference types.

use crate::hash::Hash;

/// A lightweight pointer to a commit in the DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// Branch name (e.g. `"main"`, `"feature/foo"`).
    pub name: String,
    /// Hash of the commit this branch currently points to.
    pub target: Hash,
}
