//! Runtime values and execution results.

use std::collections::BTreeMap;
use std::fmt;

/// A runtime value produced by queries or used in bindings.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Floating point.
    Float(f64),
    /// UTF-8 string (also used for hashes as hex).
    String(String),
    /// Ordered map (commit fields, HAMT projections).
    Map(BTreeMap<String, Value>),
}

impl Value {
    /// Decode HAMT bytes as UTF-8 string (lossy).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::String(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Encode a literal-ish value for HAMT storage.
    pub fn to_storage_bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Null => Ok(Vec::new()),
            Self::Bool(b) => Ok(if *b {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }),
            Self::Int(n) => Ok(n.to_string().into_bytes()),
            Self::Float(n) => Ok(n.to_string().into_bytes()),
            Self::String(s) => Ok(s.as_bytes().to_vec()),
            Self::Map(_) => Err("cannot store nested maps as a single HAMT value yet".into()),
        }
    }

    /// Truthiness for WHERE filters.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Int(n) => *n != 0,
            Self::Float(n) => *n != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::Map(m) => !m.is_empty(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "{s}"),
            Self::Map(m) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in m {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

/// Tabular query result.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResult {
    /// Column names (aliases or expression labels).
    pub columns: Vec<String>,
    /// Row values aligned with `columns`.
    pub rows: Vec<Vec<Value>>,
}

impl QueryResult {
    /// Empty result set.
    pub fn empty() -> Self {
        Self {
            columns: vec![],
            rows: vec![],
        }
    }
}

/// Outcome of a mutating statement.
#[derive(Clone, Debug, PartialEq)]
pub struct MutationResult {
    /// Human-readable summary.
    pub summary: String,
    /// Optional primary commit / object hash affected.
    pub hash: Option<String>,
}

/// Result of executing one statement.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecResult {
    /// Query rows.
    Query(QueryResult),
    /// Mutation acknowledgement.
    Mutation(MutationResult),
}

impl fmt::Display for ExecResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query(q) => {
                if q.columns.is_empty() {
                    return write!(f, "(no columns)");
                }
                writeln!(f, "{}", q.columns.join(" | "))?;
                writeln!(f, "{}", "-".repeat(q.columns.join(" | ").len()))?;
                for row in &q.rows {
                    let cells: Vec<String> = row.iter().map(ToString::to_string).collect();
                    writeln!(f, "{}", cells.join(" | "))?;
                }
                write!(f, "({} row(s))", q.rows.len())
            }
            Self::Mutation(m) => {
                write!(f, "{}", m.summary)?;
                if let Some(h) = &m.hash {
                    write!(f, " [{h}]")?;
                }
                Ok(())
            }
        }
    }
}
