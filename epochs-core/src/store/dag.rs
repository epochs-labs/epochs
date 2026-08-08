//! DAG store trait.

use crate::branch::Branch;
use crate::commit::Commit;
use crate::error::Result;
use crate::hash::Hash;

/// Key-value mutation applied during commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HamtOp {
    /// Insert or update a key.
    Put {
        /// Key bytes.
        key: Vec<u8>,
        /// Value bytes.
        value: Vec<u8>,
    },
}

/// High-level interface for the version-controlled DAG.
pub trait DagStore {
    /// Apply HAMT ops on top of `root`, create commit with `parents`, return commit hash.
    fn commit(
        &mut self,
        parents: Vec<Hash>,
        root: Option<Hash>,
        ops: &[HamtOp],
        message: &str,
    ) -> Result<Hash>;

    /// Create commit with an explicit HAMT root hash.
    fn commit_with_root(
        &mut self,
        parents: Vec<Hash>,
        root_hamt: Hash,
        message: &str,
    ) -> Result<Hash>;

    /// Read a key from a HAMT root.
    fn get(&mut self, root: Hash, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Load a commit by hash.
    fn get_commit(&mut self, hash: &Hash) -> Result<Commit>;

    /// Create a branch pointing at `target`.
    fn create_branch(&mut self, name: &str, target: Hash) -> Result<()>;

    /// Move branch tip to `target`.
    fn update_branch(&mut self, name: &str, target: Hash) -> Result<()>;

    /// Look up a branch.
    fn get_branch(&mut self, name: &str) -> Result<Branch>;

    /// Set HEAD to branch.
    fn set_head(&mut self, branch_name: &str) -> Result<()>;

    /// Current HEAD branch.
    fn head(&mut self) -> Result<Option<Branch>>;

    /// Resolve branch name or commit hash to a commit.
    fn checkout(&mut self, target: &str) -> Result<Commit>;
}
