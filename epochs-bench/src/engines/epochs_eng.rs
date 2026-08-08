//! epochs-core: native Merkle commit DAG + HAMT state (throughput-oriented).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use epochs_core::{DagStore, DiskStore, HamtOp, Hash};

use super::{CommitId, Put, VcsBench};

/// Keep only a trailing window of the linear chain (history fast path).
/// Full history still works via the bounded commit LRU in DiskStore.
const CHAIN_WINDOW: usize = 256;

pub struct EpochsEngine {
    store: DiskStore,
    root: PathBuf,
    tip: Option<Hash>,
    tip_root: Hash,
    /// Trailing linear parent window (capped — not the full history).
    chain: Vec<Hash>,
    since_ref_flush: u32,
    ref_flush_every: u32,
}

impl EpochsEngine {
    pub fn open(dir: &Path) -> Result<Self, String> {
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let (mut store, tip) =
            DiskStore::init(dir, "main", "bench genesis").map_err(|e| e.to_string())?;
        // Bench durability ≈ SQLite WAL + NORMAL; lean caches for fair RSS.
        store.set_fsync_every(512);
        store.configure_caches(1_024, 256, 2);
        let tip_root = store.get_commit(&tip).map_err(|e| e.to_string())?.root_hamt;
        Ok(Self {
            store,
            root: dir.to_path_buf(),
            tip: Some(tip),
            tip_root,
            chain: vec![tip],
            since_ref_flush: 0,
            ref_flush_every: 512,
        })
    }

    fn parse(id: &CommitId) -> Result<Hash, String> {
        Hash::from_hex(id).map_err(|e| e.to_string())
    }

    fn flush_main_ref(&mut self) -> Result<(), String> {
        if let Some(tip) = self.tip {
            self.store
                .update_branch("main", tip)
                .map_err(|e| e.to_string())?;
        }
        self.since_ref_flush = 0;
        Ok(())
    }

    fn push_chain(&mut self, id: Hash) {
        self.chain.push(id);
        if self.chain.len() > CHAIN_WINDOW {
            let drop_n = self.chain.len() - CHAIN_WINDOW;
            self.chain.drain(0..drop_n);
        }
    }
}

impl Drop for EpochsEngine {
    fn drop(&mut self) {
        let _ = self.flush_main_ref();
        let _ = self.store.flush();
    }
}

impl VcsBench for EpochsEngine {
    fn name(&self) -> &'static str {
        "epochs"
    }

    fn commit(
        &mut self,
        parent: Option<&CommitId>,
        puts: &[Put],
        message: &str,
    ) -> Result<CommitId, String> {
        // `None` parent = extend current tip (hot path for linear scale loads).
        let (parents, base_root) = if let Some(p) = parent {
            let h = Self::parse(p)?;
            let root = if self.tip == Some(h) {
                non_zero(self.tip_root)
            } else {
                let c = self.store.get_commit(&h).map_err(|e| e.to_string())?;
                non_zero(c.root_hamt)
            };
            (vec![h], root)
        } else if let Some(t) = self.tip {
            (vec![t], non_zero(self.tip_root))
        } else {
            (vec![], None)
        };

        let ops: Vec<HamtOp> = puts
            .iter()
            .map(|p| HamtOp::Put {
                key: p.key.clone(),
                value: p.value.clone(),
            })
            .collect();

        let root_hamt = self
            .store
            .apply_hamt_ops(base_root, &ops)
            .map_err(|e| e.to_string())?;
        let id = self
            .store
            .commit_with_root(parents, root_hamt, message)
            .map_err(|e| e.to_string())?;

        self.tip = Some(id);
        self.tip_root = root_hamt;
        self.push_chain(id);
        self.since_ref_flush += 1;
        if self.since_ref_flush >= self.ref_flush_every {
            self.flush_main_ref()?;
        }

        Ok(id.to_string())
    }

    fn branch(&mut self, name: &str, tip: &CommitId) -> Result<(), String> {
        self.flush_main_ref()?;
        let h = Self::parse(tip)?;
        match self.store.create_branch(name, h) {
            Ok(()) => Ok(()),
            Err(_) => self.store.update_branch(name, h).map_err(|e| e.to_string()),
        }
    }

    fn history(&mut self, tip: &CommitId, depth: u32) -> Result<Vec<CommitId>, String> {
        let tip_h = Self::parse(tip)?;
        // Fast path: tip is end of capped linear window and depth fits.
        if self.chain.last() == Some(&tip_h) && (depth as usize) <= self.chain.len() {
            let n = depth as usize;
            let start = self.chain.len().saturating_sub(n);
            let mut out = Vec::with_capacity(n);
            for h in self.chain[start..].iter().rev() {
                out.push(h.to_string());
            }
            return Ok(out);
        }

        // General path: parent links via bounded commit LRU / CAS (always correct).
        let mut out = Vec::with_capacity(depth as usize);
        let mut cur = tip_h;
        for _ in 0..depth {
            out.push(cur.to_string());
            let c = self.store.get_commit(&cur).map_err(|e| e.to_string())?;
            match c.parents.first() {
                Some(p) => cur = *p,
                None => break,
            }
        }
        Ok(out)
    }

    fn checkout(&mut self, commit: &CommitId) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        self.flush_main_ref()?;
        let h = Self::parse(commit)?;
        let c = self.store.get_commit(&h).map_err(|e| e.to_string())?;
        let entries = self
            .store
            .hamt_entries(c.root_hamt)
            .map_err(|e| e.to_string())?;
        Ok(entries.into_iter().collect())
    }

    fn disk_bytes(&mut self) -> Result<u64, String> {
        self.flush_main_ref()?;
        self.store.flush().map_err(|e| e.to_string())?;
        dir_size(&self.root)
    }
}

fn non_zero(h: Hash) -> Option<Hash> {
    if h == Hash::ZERO {
        None
    } else {
        Some(h)
    }
}

fn dir_size(path: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    if !path.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += meta.len();
        }
    }
    Ok(total)
}
