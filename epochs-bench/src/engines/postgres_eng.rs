//! Postgres commit-DAG peer (same logical schema as SQLite).

use std::collections::BTreeMap;

use postgres::{Client, NoTls};

use super::{CommitId, Put, VcsBench};

pub struct PostgresEngine {
    client: Client,
    tip: Option<i64>,
}

impl PostgresEngine {
    pub fn open(url: &str) -> Result<Self, String> {
        let mut client = Client::connect(url, NoTls).map_err(|e| {
            format!("postgres: {e} — use ./benches/run.sh (fair Docker) or pass --postgres-url")
        })?;
        client
            .batch_execute(
                "
                DROP TABLE IF EXISTS commit_ops;
                DROP TABLE IF EXISTS branches;
                DROP TABLE IF EXISTS commits;
                CREATE TABLE commits (
                    id BIGSERIAL PRIMARY KEY,
                    parent_id BIGINT REFERENCES commits(id),
                    message TEXT NOT NULL,
                    ts BIGINT NOT NULL
                );
                CREATE TABLE commit_ops (
                    commit_id BIGINT NOT NULL REFERENCES commits(id),
                    key BYTEA NOT NULL,
                    value BYTEA NOT NULL,
                    PRIMARY KEY (commit_id, key)
                );
                CREATE TABLE branches (
                    name TEXT PRIMARY KEY,
                    tip_id BIGINT NOT NULL REFERENCES commits(id)
                );
                CREATE INDEX idx_commits_parent ON commits(parent_id);
                ",
            )
            .map_err(|e| e.to_string())?;
        Ok(Self { client, tip: None })
    }

    fn replay(&mut self, tip: i64) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let mut chain = Vec::new();
        let mut cur = Some(tip);
        while let Some(id) = cur {
            chain.push(id);
            let row = self
                .client
                .query_opt("SELECT parent_id FROM commits WHERE id = $1", &[&id])
                .map_err(|e| e.to_string())?;
            cur = row.and_then(|r| r.get::<_, Option<i64>>(0));
        }
        chain.reverse();

        let mut state = BTreeMap::new();
        for id in chain {
            let rows = self
                .client
                .query(
                    "SELECT key, value FROM commit_ops WHERE commit_id = $1",
                    &[&id],
                )
                .map_err(|e| e.to_string())?;
            for row in rows {
                state.insert(row.get(0), row.get(1));
            }
        }
        Ok(state)
    }
}

impl VcsBench for PostgresEngine {
    fn name(&self) -> &'static str {
        "postgres"
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

        let row = self
            .client
            .query_one(
                "INSERT INTO commits (parent_id, message, ts) VALUES ($1, $2, $3) RETURNING id",
                &[&parent_id, &message, &ts],
            )
            .map_err(|e| e.to_string())?;
        let id: i64 = row.get(0);

        for put in puts {
            self.client
                .execute(
                    "INSERT INTO commit_ops (commit_id, key, value) VALUES ($1, $2, $3)",
                    &[&id, &put.key.as_slice(), &put.value.as_slice()],
                )
                .map_err(|e| e.to_string())?;
        }

        self.client
            .execute(
                "INSERT INTO branches(name, tip_id) VALUES('main', $1)
                 ON CONFLICT(name) DO UPDATE SET tip_id = EXCLUDED.tip_id",
                &[&id],
            )
            .map_err(|e| e.to_string())?;

        self.tip = Some(id);
        Ok(id.to_string())
    }

    fn branch(&mut self, name: &str, tip: &CommitId) -> Result<(), String> {
        let tip_id: i64 = tip.parse().map_err(|e| format!("{e}"))?;
        self.client
            .execute(
                "INSERT INTO branches(name, tip_id) VALUES($1, $2)
                 ON CONFLICT(name) DO UPDATE SET tip_id = EXCLUDED.tip_id",
                &[&name, &tip_id],
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
            let row = self
                .client
                .query_opt("SELECT parent_id FROM commits WHERE id = $1", &[&id])
                .map_err(|e| e.to_string())?;
            cur = row.and_then(|r| r.get::<_, Option<i64>>(0));
        }
        Ok(out)
    }

    fn checkout(&mut self, commit: &CommitId) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, String> {
        let id: i64 = commit.parse().map_err(|e| format!("{e}"))?;
        self.replay(id)
    }

    fn disk_bytes(&mut self) -> Result<u64, String> {
        let row = self
            .client
            .query_one(
                "SELECT pg_total_relation_size('commits')
                      + pg_total_relation_size('commit_ops')
                      + pg_total_relation_size('branches')",
                &[],
            )
            .map_err(|e| e.to_string())?;
        Ok(row.get::<_, i64>(0) as u64)
    }
}
