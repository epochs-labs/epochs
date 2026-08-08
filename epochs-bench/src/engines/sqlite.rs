//! SQL commit-DAG schema (best practical peer for branch/history/replay).
//!
//! ```text
//! branches(name PK, tip_id)
//! commits(id PK, parent_id, message, ts)
//! commit_ops(commit_id, key, value)   -- deltas; checkout = replay chain
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use super::{CommitId, Put, VcsBench};

pub struct SqliteEngine {
    conn: Connection,
    root: PathBuf,
    tip: Option<i64>,
}

impl SqliteEngine {
    pub fn open(dir: &Path) -> Result<Self, String> {
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(dir.join("vcs.db")).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -8000;
            PRAGMA temp_store = MEMORY;
            CREATE TABLE commits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER REFERENCES commits(id),
                message TEXT NOT NULL,
                ts INTEGER NOT NULL
            );
            CREATE TABLE commit_ops (
                commit_id INTEGER NOT NULL REFERENCES commits(id),
                key BLOB NOT NULL,
                value BLOB NOT NULL,
                PRIMARY KEY (commit_id, key)
            );
            CREATE TABLE branches (
                name TEXT PRIMARY KEY,
                tip_id INTEGER NOT NULL REFERENCES commits(id)
            );
            CREATE INDEX idx_commits_parent ON commits(parent_id);
            ",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            conn,
            root: dir.to_path_buf(),
            tip: None,
        })
    }

    fn replay(&self, tip: i64) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        // Collect chain tip → root, then apply ops root → tip (LWW).
        let mut chain = Vec::new();
        let mut cur = Some(tip);
        while let Some(id) = cur {
            chain.push(id);
            cur = self
                .conn
                .query_row(
                    "SELECT parent_id FROM commits WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .map_err(|e| e.to_string())?;
        }
        chain.reverse();

        let mut state = BTreeMap::new();
        for id in chain {
            let mut stmt = self
                .conn
                .prepare("SELECT key, value FROM commit_ops WHERE commit_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for r in rows {
                let (k, v) = r.map_err(|e| e.to_string())?;
                state.insert(k, v);
            }
        }
        Ok(state)
    }
}

impl VcsBench for SqliteEngine {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn commit(
        &mut self,
        parent: Option<&CommitId>,
        puts: &[Put],
        message: &str,
    ) -> Result<CommitId, String> {
        let parent_id: Option<i64> = match parent {
            Some(p) => Some(p.parse().map_err(|e| format!("{e}"))?),
            None => self.tip,
        };
        let ts = self.tip.map(|t| t + 1).unwrap_or(1);

        self.conn
            .execute(
                "INSERT INTO commits (parent_id, message, ts) VALUES (?1, ?2, ?3)",
                params![parent_id, message, ts],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();

        for put in puts {
            self.conn
                .execute(
                    "INSERT INTO commit_ops (commit_id, key, value) VALUES (?1, ?2, ?3)",
                    params![id, put.key.as_slice(), put.value.as_slice()],
                )
                .map_err(|e| e.to_string())?;
        }

        self.conn
            .execute(
                "INSERT INTO branches(name, tip_id) VALUES('main', ?1)
                 ON CONFLICT(name) DO UPDATE SET tip_id = excluded.tip_id",
                params![id],
            )
            .map_err(|e| e.to_string())?;

        self.tip = Some(id);
        Ok(id.to_string())
    }

    fn branch(&mut self, name: &str, tip: &CommitId) -> Result<(), String> {
        let tip_id: i64 = tip.parse().map_err(|e| format!("{e}"))?;
        self.conn
            .execute(
                "INSERT INTO branches(name, tip_id) VALUES(?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET tip_id = excluded.tip_id",
                params![name, tip_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn history(&mut self, tip: &CommitId, depth: u32) -> Result<Vec<CommitId>, String> {
        let mut out = Vec::new();
        let mut cur: Option<i64> = Some(tip.parse().map_err(|e| format!("{e}"))?);
        for _ in 0..depth {
            let Some(id) = cur else { break };
            out.push(id.to_string());
            cur = self
                .conn
                .query_row(
                    "SELECT parent_id FROM commits WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .flatten();
        }
        Ok(out)
    }

    fn checkout(&mut self, commit: &CommitId) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let id: i64 = commit.parse().map_err(|e| format!("{e}"))?;
        self.replay(id)
    }

    fn disk_bytes(&mut self) -> Result<u64, String> {
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        dir_size(&self.root)
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
