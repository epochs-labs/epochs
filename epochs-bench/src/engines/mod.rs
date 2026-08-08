//! Versioned store trait: commit, branch, history walk, checkout/replay.

mod epochs_eng;
mod mysql_eng;
mod postgres_eng;
mod sqlite;

pub use epochs_eng::EpochsEngine;
pub use mysql_eng::MysqlEngine;
pub use postgres_eng::PostgresEngine;
pub use sqlite::SqliteEngine;

use std::collections::BTreeMap;
use std::time::Duration;

/// Opaque commit identifier (hex hash or decimal SQL id).
pub type CommitId = String;

/// Workload shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Bounded live keys, updates over deep history (default — git-like).
    Deep,
    /// Unique key per commit (stress / worst-case state growth).
    Wide,
}

impl Shape {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "deep" => Ok(Self::Deep),
            "wide" => Ok(Self::Wide),
            other => Err(format!("unknown shape '{other}' (deep|wide)")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deep => "deep",
            Self::Wide => "wide",
        }
    }
}

/// One key put applied in a commit.
#[derive(Clone, Debug)]
pub struct Put {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Backend that implements a **commit DAG** (not a flat table).
///
/// SQL engines use the best practical schema for this model:
/// `branches` tip pointers + `commits` parent links + `commit_ops` deltas,
/// with checkout = replay ops along the ancestor chain.
pub trait VcsBench {
    fn name(&self) -> &'static str;

    /// Create a commit with `puts` on top of `parent` (None = root).
    fn commit(
        &mut self,
        parent: Option<&CommitId>,
        puts: &[Put],
        message: &str,
    ) -> Result<CommitId, String>;

    /// Point (or create) branch `name` at `tip`.
    fn branch(&mut self, name: &str, tip: &CommitId) -> Result<(), String>;

    /// Walk up to `depth` parents from `tip` (tip first).
    fn history(&mut self, tip: &CommitId, depth: u32) -> Result<Vec<CommitId>, String>;

    /// Materialize full key→value map at `commit` (replay / checkout).
    fn checkout(&mut self, commit: &CommitId) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String>;

    /// On-disk (or relation) bytes after work.
    fn disk_bytes(&mut self) -> Result<u64, String>;
}

#[derive(Clone, Debug, Default)]
pub struct LatencyStats {
    samples_ns: Vec<u64>,
    /// When set, [`Self::count`] reports this (e.g. full commit count with sampled latencies).
    count_override: Option<usize>,
}

impl LatencyStats {
    pub fn record(&mut self, d: Duration) {
        self.samples_ns.push(d.as_nanos() as u64);
    }

    pub fn set_count_override(&mut self, n: usize) {
        self.count_override = Some(n);
    }

    pub fn count(&self) -> usize {
        self.count_override.unwrap_or(self.samples_ns.len())
    }

    pub fn percentile(&self, p: f64) -> Duration {
        if self.samples_ns.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted = self.samples_ns.clone();
        sorted.sort_unstable();
        let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
        Duration::from_nanos(sorted[idx.min(sorted.len() - 1)])
    }

    pub fn p50(&self) -> Duration {
        self.percentile(50.0)
    }

    pub fn p99(&self) -> Duration {
        self.percentile(99.0)
    }
}

#[derive(Clone, Debug)]
pub struct BenchReport {
    pub engine: String,
    pub tier: String,
    pub shape: Shape,
    pub live_keys: u64,
    pub commits: u64,
    pub w1_commit: LatencyStats,
    pub w2_branch: LatencyStats,
    pub r1_history: LatencyStats,
    pub r2_checkout: LatencyStats,
    pub disk_bytes: u64,
    pub memory_bytes: Option<u64>,
    pub load_secs: f64,
}
