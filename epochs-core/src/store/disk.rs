//! Disk-backed DAG store.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::branch::Branch;
use crate::cache::LruMap;
use crate::cas::{CasStore, RecordType};
use crate::commit::Commit;
use crate::error::{EpochsError, Result};
use crate::hamt::PersistentHamt;
use crate::hash::Hash;
use crate::repo::Repo;
use crate::store::{DagStore, HamtOp};

/// Bound on decoded-commit LRU (immutable once written).
const COMMIT_CACHE_CAP: usize = 4_096;

/// Persistent DAG store composing repo refs and CAS.
pub struct DiskStore {
    repo: Repo,
    cas: CasStore,
    /// Hot commit cache (hash → decoded commit) for history walks.
    ///
    /// Safe: commit objects are immutable. Bounded so long-running writers
    /// do not retain every historical commit in RSS.
    commit_cache: LruMap<Hash, Commit>,
    /// fsync CAS after this many commits (1 = every commit; higher ≈ SQLite WAL NORMAL).
    fsync_every: u32,
    commits_since_fsync: u32,
}

impl DiskStore {
    /// Open a repository and its CAS layer.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let repo = Repo::open(path)?;
        let cas = CasStore::open(path)?;
        Ok(Self {
            repo,
            cas,
            commit_cache: LruMap::new(COMMIT_CACHE_CAP),
            fsync_every: 1,
            commits_since_fsync: 0,
        })
    }

    /// How often to `fsync` after commits (`1` = every commit).
    ///
    /// Values like `32`–`128` approximate SQLite `synchronous=NORMAL` + WAL
    /// group commit behavior for throughput-oriented workloads.
    pub fn set_fsync_every(&mut self, n: u32) {
        self.fsync_every = n.max(1);
    }

    /// Bound decoded-commit and CAS object caches (and mmap segment count).
    ///
    /// Lower values reduce RSS; too low can hurt history/checkout locality.
    pub fn configure_caches(&mut self, object_cap: usize, commit_cap: usize, mmap_segments: usize) {
        self.cas.set_object_cache_cap(object_cap);
        self.cas.set_mmap_cap(mmap_segments);
        self.commit_cache = LruMap::new(commit_cap);
    }

    /// Force pending CAS durability now.
    pub fn flush(&mut self) -> Result<()> {
        self.cas.flush()?;
        self.commits_since_fsync = 0;
        Ok(())
    }

    /// Repository root path (where `migrations/`, `schema.state`, and CAS live).
    pub fn path(&self) -> &std::path::Path {
        self.repo.path()
    }

    /// Initialize a new repository with a genesis commit and default branch.
    pub fn init(
        path: &std::path::Path,
        default_branch: &str,
        message: &str,
    ) -> Result<(Self, Hash)> {
        let mut repo = Repo::init(path)?;
        let mut cas = CasStore::open(path)?;

        let commit_hash = Self::write_genesis(&mut cas, message)?;
        repo.create_branch(default_branch, commit_hash)?;
        repo.set_head(default_branch)?;

        let store = Self {
            repo,
            cas,
            commit_cache: LruMap::new(COMMIT_CACHE_CAP),
            fsync_every: 1,
            commits_since_fsync: 0,
        };
        Ok((store, commit_hash))
    }

    fn write_genesis(cas: &mut CasStore, message: &str) -> Result<Hash> {
        let commit = Commit::new(vec![], Hash::ZERO, now_millis(), message);
        let payload = commit.encode_payload();
        let hash = cas.put(RecordType::Commit, &payload)?;
        cas.flush()?;
        Ok(hash)
    }

    fn resolve_target(&mut self, target: &str) -> Result<Hash> {
        if target.eq_ignore_ascii_case("HEAD") {
            let head = self
                .head()?
                .ok_or_else(|| EpochsError::InvalidTarget("HEAD not set".into()))?;
            return Ok(head.target);
        }

        if let Ok(branch) = self.repo.read_branch(target) {
            return Ok(branch.target);
        }

        self.resolve_hash_ref(target)
    }

    /// Resolve a full hash or unambiguous hex prefix to a stored object hash.
    pub fn resolve_hash_ref(&self, hex: &str) -> Result<Hash> {
        if hex.len() == 64 {
            let hash = Hash::from_hex(hex)?;
            if self.cas.contains(&hash) {
                return Ok(hash);
            }
            return Err(EpochsError::CommitNotFound(hash));
        }

        let matches = self.cas.find_by_prefix(hex);
        match matches.len() {
            0 => Err(EpochsError::InvalidTarget(format!(
                "no object matches prefix '{hex}'"
            ))),
            1 => Ok(matches[0]),
            _ => Err(EpochsError::InvalidTarget(format!(
                "ambiguous hash prefix '{hex}' ({} matches)",
                matches.len()
            ))),
        }
    }

    fn load_commit(&mut self, hash: &Hash) -> Result<Commit> {
        if let Some(c) = self.commit_cache.get(hash) {
            return Ok(c.clone());
        }
        let record = self.cas.get_record(hash)?;
        if record.record_type != RecordType::Commit {
            return Err(EpochsError::Codec("hash is not a commit".into()));
        }
        let commit = Commit::decode_payload(&record.payload)?;
        self.commit_cache.insert(*hash, commit.clone());
        Ok(commit)
    }

    fn persist_commit(&mut self, commit: &Commit) -> Result<Hash> {
        let payload = commit.encode_payload();
        let hash = self.cas.put(RecordType::Commit, &payload)?;
        self.commit_cache.insert(hash, commit.clone());
        self.commits_since_fsync += 1;
        if self.commits_since_fsync >= self.fsync_every {
            self.cas.flush()?;
            self.commits_since_fsync = 0;
        }
        Ok(hash)
    }

    /// Return the CAS hash of the HAMT leaf storing `key`, if present.
    pub fn hamt_leaf_hash(&mut self, root: Hash, key: &[u8]) -> Result<Option<Hash>> {
        if root == Hash::ZERO {
            return Ok(None);
        }
        PersistentHamt::leaf_hash(&mut self.cas, Some(root), key)
    }

    /// List all key-value pairs in a HAMT root.
    pub fn hamt_entries(&mut self, root: Hash) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        if root == Hash::ZERO {
            return Ok(vec![]);
        }
        PersistentHamt::entries(&mut self.cas, Some(root))
    }

    /// Delete a branch (cannot delete current HEAD).
    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        self.repo.delete_branch(name)
    }

    /// List branch names.
    pub fn list_branches(&self) -> Result<Vec<String>> {
        self.repo.list_branches()
    }

    /// Collect all commits reachable from every branch tip.
    pub fn reachable_commits(&mut self) -> Result<HashSet<Hash>> {
        let mut all = HashSet::new();
        for name in self.list_branches()? {
            let tip = self.get_branch(&name)?.target;
            let mut queue = VecDeque::from([tip]);
            while let Some(h) = queue.pop_front() {
                if !all.insert(h) {
                    continue;
                }
                let commit = self.load_commit(&h)?;
                for p in commit.parents {
                    queue.push_back(p);
                }
            }
        }
        Ok(all)
    }

    /// Build parent → children map over reachable commits.
    pub fn children_map(&mut self) -> Result<HashMap<Hash, Vec<Hash>>> {
        let commits = self.reachable_commits()?;
        let mut map: HashMap<Hash, Vec<Hash>> = HashMap::new();
        for hash in &commits {
            let commit = self.load_commit(hash)?;
            for parent in commit.parents {
                map.entry(parent).or_default().push(*hash);
            }
        }
        Ok(map)
    }

    /// Advance the named branch to `commit` after verifying it exists.
    pub fn advance_branch(&mut self, name: &str, commit: Hash) -> Result<()> {
        self.update_branch(name, commit)
    }

    /// Create a commit that includes secondary `index_roots`.
    pub fn commit_with_indexes(
        &mut self,
        parents: Vec<Hash>,
        root_hamt: Hash,
        index_roots: std::collections::BTreeMap<String, Hash>,
        message: &str,
    ) -> Result<Hash> {
        for parent in &parents {
            if !self.cas.contains(parent) {
                return Err(EpochsError::InvalidCommit(format!(
                    "parent not found: {parent}"
                )));
            }
        }
        let commit = Commit::with_indexes(parents, root_hamt, now_millis(), message, index_roots);
        self.persist_commit(&commit)
    }

    /// Apply HAMT put ops and return the new root without creating a commit.
    pub fn apply_hamt_ops(&mut self, root: Option<Hash>, ops: &[HamtOp]) -> Result<Hash> {
        let mut current_root = root;
        for op in ops {
            let HamtOp::Put { key, value } = op;
            let base = if current_root == Some(Hash::ZERO) {
                None
            } else {
                current_root
            };
            current_root = Some(PersistentHamt::insert(&mut self.cas, base, key, value)?);
        }
        Ok(current_root.unwrap_or(Hash::ZERO))
    }

    /// Insert a key into an index HAMT root.
    pub fn index_put(
        &mut self,
        index_root: Option<Hash>,
        key: &[u8],
        value: &[u8],
    ) -> Result<Hash> {
        PersistentHamt::insert(&mut self.cas, index_root, key, value)
    }

    /// Remove a key from an index HAMT root.
    pub fn index_remove(&mut self, index_root: Option<Hash>, key: &[u8]) -> Result<Option<Hash>> {
        PersistentHamt::remove(&mut self.cas, index_root, key)
    }

    /// Look up a value in an index HAMT.
    pub fn index_get(&mut self, index_root: Hash, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if index_root == Hash::ZERO {
            return Ok(None);
        }
        PersistentHamt::get(&mut self.cas, Some(index_root), key)
    }
}

impl DagStore for DiskStore {
    fn commit(
        &mut self,
        parents: Vec<Hash>,
        root: Option<Hash>,
        ops: &[HamtOp],
        message: &str,
    ) -> Result<Hash> {
        let mut current_root = root;
        for op in ops {
            let HamtOp::Put { key, value } = op;
            let base = if current_root == Some(Hash::ZERO) {
                None
            } else {
                current_root
            };
            current_root = Some(PersistentHamt::insert(&mut self.cas, base, key, value)?);
        }
        let root_hamt = current_root.unwrap_or(Hash::ZERO);
        self.commit_with_root(parents, root_hamt, message)
    }

    fn commit_with_root(
        &mut self,
        parents: Vec<Hash>,
        root_hamt: Hash,
        message: &str,
    ) -> Result<Hash> {
        for parent in &parents {
            if !self.cas.contains(parent) {
                return Err(EpochsError::InvalidCommit(format!(
                    "parent not found: {parent}"
                )));
            }
        }
        let commit = Commit::new(parents, root_hamt, next_timestamp(), message);
        self.persist_commit(&commit)
    }

    fn get(&mut self, root: Hash, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if root == Hash::ZERO {
            return Ok(None);
        }
        PersistentHamt::get(&mut self.cas, Some(root), key)
    }

    fn get_commit(&mut self, hash: &Hash) -> Result<Commit> {
        self.load_commit(hash)
    }

    fn create_branch(&mut self, name: &str, target: Hash) -> Result<()> {
        if !self.cas.contains(&target) {
            return Err(EpochsError::CommitNotFound(target));
        }
        self.repo.create_branch(name, target)
    }

    fn update_branch(&mut self, name: &str, target: Hash) -> Result<()> {
        if !self.cas.contains(&target) {
            return Err(EpochsError::CommitNotFound(target));
        }
        self.repo.update_branch(name, target)
    }

    fn get_branch(&mut self, name: &str) -> Result<Branch> {
        self.repo.read_branch(name)
    }

    fn set_head(&mut self, branch_name: &str) -> Result<()> {
        self.repo.set_head(branch_name)
    }

    fn head(&mut self) -> Result<Option<Branch>> {
        match self.repo.head_branch().map(str::to_owned) {
            Some(name) => Ok(Some(self.get_branch(&name)?)),
            None => Ok(None),
        }
    }

    fn checkout(&mut self, target: &str) -> Result<Commit> {
        let hash = self.resolve_target(target)?;
        self.load_commit(&hash)
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Monotonic timestamps without a syscall on every commit.
fn next_timestamp() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TS: AtomicU64 = AtomicU64::new(0);
    let mut n = TS.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        n = now_millis();
        TS.store(n + 1, Ordering::Relaxed);
        return n;
    }
    n
}
