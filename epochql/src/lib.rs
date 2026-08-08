//! # EpochQL
//!
//! Context-aware, pattern-matching query language for Merkle-DAG state traversal,
//! time travel, and branch management. Combines Cypher-style graph patterns with
//! version-control primitives (`USE`, `COMMIT`, `MERGE`, `DIFF`).
//!
//! EpochQL sits above [`epochs_core`] and is the primary query interface for
//! application layers such as agent conversation storage.
//!
//! # Example
//!
//! ```no_run
//! use epochs_core::DiskStore;
//! use epochql::Engine;
//!
//! let mut store = DiskStore::open(std::path::Path::new(".epochs")).unwrap();
//! let mut engine = Engine::new(&mut store);
//! let results = engine.execute(r#"
//!     CREATE BRANCH experiment FROM HEAD;
//!     COMMIT { status: "running" } MESSAGE "go";
//! "#).unwrap();
//! ```
//!
//! # Modules
//!
//! - [`ast`] — abstract syntax tree types
//! - [`lexer`] — tokenizer
//! - [`parser`] — recursive-descent parser
//! - [`exec`] — schemaless executor ([`Engine`])
//! - [`schema`] — `.eql` migrations, registry, path indexes
//! - [`error`] — [`ParseError`]

#![warn(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod ast;
pub mod error;
pub mod exec;
pub mod lexer;
pub mod parser;
pub mod schema;

pub use ast::{
    BinaryOp, BranchStmt, CommitStmt, ContextClause, DiffStmt, EdgeDirection, EdgePattern,
    EdgeType, Expression, HopMultiplier, MatchClause, MergeStmt, MergeStrategy, NodePattern,
    Pattern, PatternElement, ProjectionItem, QueryStatement, SelectClause, Statement, TargetRef,
    TraversalClause, VersionStatement,
};
pub use error::{ParseError, Result};
pub use exec::{Engine, ExecError, ExecResult, MutationResult, QueryResult, Value};
pub use parser::{parse, parse_script};
pub use schema::{migrate, MigrationReport, SchemaRegistry};

/// EpochQL language / crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
