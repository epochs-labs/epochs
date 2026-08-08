//! MySQL commit-DAG peer (same logical schema as SQLite/Postgres).

use std::collections::BTreeMap;

use mysql::prelude::*;
use mysql::{Opts, Pool, PooledConn};

use super::{CommitId, Put, VcsBench};

pub struct MysqlEngine {
    conn: PooledConn,
    tip: Option<i64>,
}

impl MysqlEngine {
    pub fn open(url: &str) -> Result<Self, String> {
        let opts = Opts::from_url(url).map_err(|e| {
            format!("mysql url: {e} — use ./benches/run.sh (fair Docker) or pass --mysql-url")
        })?;
        let pool = Pool::new(opts).map_err(|e| format!("mysql pool: {e}"))?;
        let mut conn = pool.get_conn().map_err(|e| format!("mysql connect: {e}"))?;

        conn.query_drop("SET NAMES utf8mb4")
            .map_err(|e| e.to_string())?;
        conn.query_drop("DROP TABLE IF EXISTS commit_ops")
            .map_err(|e| e.to_string())?;
        conn.query_drop("DROP TABLE IF EXISTS branches")
            .map_err(|e| e.to_string())?;
        conn.query_drop("DROP TABLE IF EXISTS commits")
            .map_err(|e| e.to_string())?;
        conn.query_drop(
            "CREATE TABLE commits (
                id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
                parent_id BIGINT NULL,
                message TEXT NOT NULL,
                ts BIGINT NOT NULL,
                INDEX idx_commits_parent (parent_id)
            ) ENGINE=InnoDB",
        )
        .map_err(|e| e.to_string())?;
        conn.query_drop(
            "CREATE TABLE commit_ops (
                commit_id BIGINT NOT NULL,
                `key` VARBINARY(512) NOT NULL,
                value LONGBLOB NOT NULL,
                PRIMARY KEY (commit_id, `key`),
                CONSTRAINT fk_ops_commit FOREIGN KEY (commit_id) REFERENCES commits(id)
            ) ENGINE=InnoDB",
        )
        .map_err(|e| e.to_string())?;
        conn.query_drop(
            "CREATE TABLE branches (
                name VARCHAR(255) NOT NULL PRIMARY KEY,
                tip_id BIGINT NOT NULL,
                CONSTRAINT fk_branch_tip FOREIGN KEY (tip_id) REFERENCES commits(id)
            ) ENGINE=InnoDB",
        )
        .map_err(|e| e.to_string())?;

        Ok(Self { conn, tip: None })
    }

    fn replay(&mut self, tip: i64) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let mut chain = Vec::new();
        let mut cur = Some(tip);
        while let Some(id) = cur {
            chain.push(id);
            let parent: Option<Option<i64>> = self
                .conn
                .exec_first("SELECT parent_id FROM commits WHERE id = ?", (id,))
                .map_err(|e| e.to_string())?;
            cur = parent.flatten();
        }
        chain.reverse();

        let mut state = BTreeMap::new();
        for id in chain {
            let rows: Vec<(Vec<u8>, Vec<u8>)> = self
                .conn
                .exec(
                    "SELECT `key`, value FROM commit_ops WHERE commit_id = ?",
                    (id,),
                )
                .map_err(|e| e.to_string())?;
            for (k, v) in rows {
                state.insert(k, v);
            }
        }
        Ok(state)
    }
}

impl VcsBench for MysqlEngine {
    fn name(&self) -> &'static str {
        "mysql"
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
            .exec_drop(
                "INSERT INTO commits (parent_id, message, ts) VALUES (?, ?, ?)",
                (parent_id, message, ts),
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_id() as i64;

        for put in puts {
            self.conn
                .exec_drop(
                    "INSERT INTO commit_ops (commit_id, `key`, value) VALUES (?, ?, ?)",
                    (id, put.key.as_slice(), put.value.as_slice()),
                )
                .map_err(|e| e.to_string())?;
        }

        self.conn
            .exec_drop(
                "INSERT INTO branches(name, tip_id) VALUES('main', ?)
                 ON DUPLICATE KEY UPDATE tip_id = VALUES(tip_id)",
                (id,),
            )
            .map_err(|e| e.to_string())?;

        self.tip = Some(id);
        Ok(id.to_string())
    }

    fn branch(&mut self, name: &str, tip: &CommitId) -> Result<(), String> {
        let tip_id: i64 = tip.parse().map_err(|e| format!("{e}"))?;
        self.conn
            .exec_drop(
                "INSERT INTO branches(name, tip_id) VALUES(?, ?)
                 ON DUPLICATE KEY UPDATE tip_id = VALUES(tip_id)",
                (name, tip_id),
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
            let parent: Option<Option<i64>> = self
                .conn
                .exec_first("SELECT parent_id FROM commits WHERE id = ?", (id,))
                .map_err(|e| e.to_string())?;
            cur = parent.flatten();
        }
        Ok(out)
    }

    fn checkout(&mut self, commit: &CommitId) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let id: i64 = commit.parse().map_err(|e| format!("{e}"))?;
        self.replay(id)
    }

    fn disk_bytes(&mut self) -> Result<u64, String> {
        let row: Option<(u64,)> = self
            .conn
            .query_first(
                "SELECT COALESCE(SUM(data_length + index_length), 0)
                 FROM information_schema.tables
                 WHERE table_schema = DATABASE()
                   AND table_name IN ('commits', 'commit_ops', 'branches')",
            )
            .map_err(|e| e.to_string())?;
        Ok(row.map(|r| r.0).unwrap_or(0))
    }
}
