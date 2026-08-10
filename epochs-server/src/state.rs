//! Shared store helpers and JSON conversion for Epoch Protocol.

use std::path::Path;
use std::sync::Arc;

use epochql::{ExecResult, MutationResult, QueryResult, Value};
use epochs_core::{DagStore, DiskStore};
use serde::Serialize;
use tokio::sync::Mutex;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<DiskStore>>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JsonExecResult {
    Query {
        columns: Vec<String>,
        rows: Vec<Vec<serde_json::Value>>,
    },
    Mutation {
        summary: String,
        hash: Option<String>,
    },
}

pub fn open_or_init(path: &Path, branch: &str) -> Result<DiskStore, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path)?;
    if path.join("HEAD").exists() || path.join("index.epi").exists() {
        info!(data = %path.display(), "opening existing repo");
        Ok(DiskStore::open(path)?)
    } else {
        info!(data = %path.display(), %branch, "initializing new repo");
        let (store, _) = DiskStore::init(path, branch, "epochs-server genesis")?;
        Ok(store)
    }
}

pub fn run_query(store: &mut DiskStore, sql: &str) -> Result<Vec<JsonExecResult>, String> {
    let mut engine = epochql::Engine::new(store);
    let results = engine.execute(sql).map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(json_exec_result).collect())
}

pub fn list_branch_tips(store: &mut DiskStore) -> Result<Vec<(String, String)>, String> {
    let names = store.list_branches().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let tip = store.get_branch(&name).map_err(|e| e.to_string())?;
        out.push((name, tip.target.to_string()));
    }
    Ok(out)
}

fn json_exec_result(r: ExecResult) -> JsonExecResult {
    match r {
        ExecResult::Query(QueryResult { columns, rows }) => JsonExecResult::Query {
            columns,
            rows: rows
                .into_iter()
                .map(|row| row.iter().map(value_to_json).collect())
                .collect(),
        },
        ExecResult::Mutation(MutationResult { summary, hash }) => {
            JsonExecResult::Mutation { summary, hash }
        }
    }
}

pub fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(n),
        Value::Float(n) => serde_json::json!(n),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Map(m) => {
            let mut map = serde_json::Map::new();
            for (k, v) in m {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}
