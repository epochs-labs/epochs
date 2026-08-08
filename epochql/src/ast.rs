//! EpochQL abstract syntax tree (grammar v1.0).
//!
//! The AST is produced by the [`crate::parser`] and consumed by the (future)
//! executor. Types are intentionally serde-free to keep `epochql` lean.

use std::collections::BTreeMap;

/// A complete EpochQL statement, terminated by `;`.
#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    /// Read-only or analytical query.
    Query(QueryStatement),
    /// Version-control or state mutation operation.
    Version(VersionStatement),
}

/// Query statement: optional context, match, traversal, filter, and projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryStatement {
    /// Execution context (`USE BRANCH`, `USE COMMIT`, etc.).
    pub context: Option<ContextClause>,
    /// Cypher-style graph pattern matching.
    pub match_clause: Option<MatchClause>,
    /// High-level graph algorithms (merge base, shortest path, etc.).
    pub traversal: Option<TraversalClause>,
    /// Row filter expression.
    pub where_clause: Option<Expression>,
    /// Result projection.
    pub select: Option<SelectClause>,
}

/// Anchors query execution to a branch, commit, tag, or HEAD.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextClause {
    /// Target reference for the execution context.
    pub target: TargetRef,
}

/// Resolves to a specific point in the version-controlled DAG.
///
/// Commit hashes are stored as hex strings (full 64-char or shorter prefixes).
/// The executor resolves prefixes against the store.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetRef {
    /// Current branch HEAD.
    Head,
    /// Named branch tip.
    Branch(String),
    /// Commit hash hex (full or prefix).
    Commit(String),
    /// Named tag (future).
    Tag(String),
}

/// `MATCH` clause containing one or more graph patterns.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchClause {
    /// Patterns to match against the DAG.
    pub patterns: Vec<Pattern>,
}

/// A linear sequence of node and edge patterns.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    /// Alternating nodes and edges starting with a node.
    pub elements: Vec<PatternElement>,
}

/// Element within a graph pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum PatternElement {
    /// Node pattern.
    Node(NodePattern),
    /// Directed edge pattern.
    Edge(EdgePattern),
}

/// Node pattern: optional variable, label, and property filter.
#[derive(Clone, Debug, PartialEq)]
pub struct NodePattern {
    /// Bound variable name.
    pub variable: Option<String>,
    /// Node label (e.g. `Commit`, `State`).
    pub label: Option<String>,
    /// Property equality filter.
    pub properties: BTreeMap<String, Expression>,
}

/// Edge pattern connecting two node patterns.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePattern {
    /// Traversal direction.
    pub direction: EdgeDirection,
    /// Semantic edge type.
    ///
    /// Bare `->` defaults to [`EdgeType::Child`]; bare `<-` defaults to
    /// [`EdgeType::Parent`].
    pub edge_type: EdgeType,
    /// Hop multiplier (e.g. `*1..5`).
    pub multiplier: Option<HopMultiplier>,
}

/// Edge traversal direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeDirection {
    /// Outgoing (parent to child / forward along the edge label).
    Outgoing,
    /// Incoming (child to parent / reverse).
    Incoming,
}

/// Semantic relationship between commits in the DAG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeType {
    /// Direct parent (one hop toward root).
    Parent,
    /// Direct child (one hop toward leaves).
    Child,
    /// Transitive ancestor relationship.
    Ancestor,
    /// Transitive descendant relationship.
    Descendant,
}

/// Hop count or range for transitive edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopMultiplier {
    /// Unbounded (`*`).
    Any,
    /// Fixed hop count (`*N`).
    Exact(u32),
    /// Inclusive range (`*N..M`).
    Range {
        /// Minimum hops (inclusive).
        min: Option<u32>,
        /// Maximum hops (inclusive).
        max: Option<u32>,
    },
}

/// Algorithmic traversal beyond pattern matching.
#[derive(Clone, Debug, PartialEq)]
pub enum TraversalClause {
    /// Find the nearest common ancestor of two targets.
    MergeBase(TargetRef, TargetRef),
    /// Find shortest path between two targets.
    ShortestPath(TargetRef, TargetRef),
    /// Collect all ancestors of a target.
    Ancestors(TargetRef),
}

/// `SELECT` projection list.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectClause {
    /// Items to project into the result set.
    pub items: Vec<ProjectionItem>,
}

/// Single projection item with optional alias.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionItem {
    /// Expression to evaluate.
    pub expression: Expression,
    /// Optional result column alias.
    pub alias: Option<String>,
}

/// Version-control and state mutation statements.
#[derive(Clone, Debug, PartialEq)]
pub enum VersionStatement {
    /// Create a new commit.
    Commit(CommitStmt),
    /// Create or delete a branch.
    Branch(BranchStmt),
    /// Move HEAD to a target.
    Checkout(TargetRef),
    /// Merge one branch/commit into another.
    Merge(MergeStmt),
    /// Diff payloads between two targets.
    Diff(DiffStmt),
}

/// `COMMIT` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitStmt {
    /// Payload properties.
    pub payload: BTreeMap<String, Expression>,
    /// Optional commit message (stored in metadata).
    pub message: Option<String>,
    /// Explicit parent list (defaults to current HEAD when executing).
    pub parents: Option<Vec<TargetRef>>,
}

/// Branch create/delete statement.
#[derive(Clone, Debug, PartialEq)]
pub enum BranchStmt {
    /// `CREATE BRANCH name FROM target`.
    Create {
        /// Branch name.
        name: String,
        /// Source target (defaults to HEAD).
        from: Option<TargetRef>,
    },
    /// `DELETE BRANCH name`.
    Delete {
        /// Branch name to remove.
        name: String,
    },
}

/// `MERGE` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeStmt {
    /// Source branch or commit.
    pub source: TargetRef,
    /// Destination branch or commit.
    pub into: TargetRef,
    /// Merge strategy.
    pub strategy: MergeStrategy,
}

/// Merge strategy selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Move destination pointer if source is a direct descendant.
    #[default]
    FastForward,
    /// Three-way merge using merge base.
    ThreeWay,
    /// Squash source commits into a single new commit.
    Squash,
}

/// `DIFF` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffStmt {
    /// Left comparison target.
    pub left: TargetRef,
    /// Right comparison target.
    pub right: TargetRef,
    /// Optional sub-path within the payload (JSON-pointer style).
    pub path: Option<String>,
}

/// Binary operators in filter / projection expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    /// Equality (`=`).
    Eq,
    /// Inequality (`!=`).
    Neq,
    /// Logical AND.
    And,
    /// Logical OR.
    Or,
}

/// Expression tree for filters, projections, and payload values.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    /// Null literal.
    Null,
    /// Boolean literal.
    Bool(bool),
    /// Integer literal.
    Int(i64),
    /// Floating-point literal.
    Float(f64),
    /// String literal.
    String(String),
    /// Variable reference.
    Variable(String),
    /// Property access (e.g. `n.hash`).
    PropertyAccess {
        /// Object expression.
        object: Box<Expression>,
        /// Property name.
        property: String,
    },
    /// Binary operation (e.g. `n.hash = "abc"`).
    Binary {
        /// Left operand.
        left: Box<Expression>,
        /// Operator.
        op: BinaryOp,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Map literal.
    Map(BTreeMap<String, Expression>),
}
