//! EpochQL executor (schema-aware commits when migrations are applied).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use epochs_core::{is_ancestor, merge_base, Commit, DagStore, DiskStore, HamtOp, Hash};

use crate::ast::*;
use crate::error::ParseError;
use crate::exec::value::{ExecResult, MutationResult, QueryResult, Value};
use crate::parser::{parse, parse_script};
use crate::schema::{doc_from_hamt_root, update_indexes_for_commit, SchemaRegistry};

/// Errors from execution (wraps store + logic errors).
#[derive(Debug)]
pub struct ExecError(pub String);

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExecError {}

impl From<epochs_core::EpochsError> for ExecError {
    fn from(e: epochs_core::EpochsError) -> Self {
        Self(e.to_string())
    }
}

impl From<ParseError> for ExecError {
    fn from(e: ParseError) -> Self {
        Self(e.to_string())
    }
}

impl From<String> for ExecError {
    fn from(e: String) -> Self {
        Self(e)
    }
}

impl From<&str> for ExecError {
    fn from(e: &str) -> Self {
        Self(e.into())
    }
}

type ExecResultType<T> = std::result::Result<T, ExecError>;

/// Bound graph node during MATCH.
#[derive(Clone, Debug)]
struct BoundCommit {
    hash: Hash,
    commit: Commit,
    /// HAMT entries as string map (utf-8 lossy).
    state: BTreeMap<String, Value>,
}

/// EpochQL execution engine over a [`DiskStore`].
pub struct Engine<'a> {
    store: &'a mut DiskStore,
}

impl<'a> Engine<'a> {
    /// Create an engine over an open store.
    pub fn new(store: &'a mut DiskStore) -> Self {
        Self { store }
    }

    /// Parse and execute a script (one or more statements).
    pub fn execute(&mut self, source: &str) -> ExecResultType<Vec<ExecResult>> {
        let stmts = parse_script(source)?;
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in &stmts {
            out.push(self.execute_statement(stmt)?);
        }
        Ok(out)
    }

    /// Parse and execute a single statement.
    pub fn execute_one(&mut self, source: &str) -> ExecResultType<ExecResult> {
        let stmt = parse(source)?;
        self.execute_statement(&stmt)
    }

    /// Execute a parsed statement.
    pub fn execute_statement(&mut self, stmt: &Statement) -> ExecResultType<ExecResult> {
        match stmt {
            Statement::Query(q) => Ok(ExecResult::Query(self.exec_query(q)?)),
            Statement::Version(v) => Ok(ExecResult::Mutation(self.exec_version(v)?)),
        }
    }

    // -------------------------------------------------------------------------
    // Target resolution
    // -------------------------------------------------------------------------

    fn resolve_target_ref(&mut self, target: &TargetRef) -> ExecResultType<Hash> {
        match target {
            TargetRef::Head => {
                let head = self
                    .store
                    .head()?
                    .ok_or_else(|| ExecError("HEAD not set".into()))?;
                Ok(head.target)
            }
            TargetRef::Branch(name) => Ok(self.store.get_branch(name)?.target),
            TargetRef::Commit(hex) => Ok(self.store.resolve_hash_ref(hex)?),
            TargetRef::Tag(name) => Err(ExecError(format!(
                "tags are not implemented yet (TAG {name})"
            ))),
        }
    }

    fn context_hash(&mut self, ctx: &Option<ContextClause>) -> ExecResultType<Hash> {
        match ctx {
            Some(c) => self.resolve_target_ref(&c.target),
            None => self.resolve_target_ref(&TargetRef::Head),
        }
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    fn exec_query(&mut self, q: &QueryStatement) -> ExecResultType<QueryResult> {
        // Pure TRAVERSE without MATCH
        if q.traversal.is_some() && q.match_clause.is_none() {
            return self.exec_traversal_only(q);
        }

        let context = self.context_hash(&q.context)?;

        if let Some(match_clause) = &q.match_clause {
            let mut rows_bindings = self.exec_match(match_clause, context)?;

            if let Some(where_expr) = &q.where_clause {
                rows_bindings
                    .retain(|bindings| self.eval_bool(where_expr, bindings).unwrap_or(false));
            }

            return self.project_rows(q.select.as_ref(), &rows_bindings);
        }

        // USE-only or SELECT against context commit
        let commit = self.store.get_commit(&context)?;
        let bound = self.bind_commit(context, commit)?;
        let mut bindings = HashMap::new();
        bindings.insert("_".into(), bound);

        if let Some(where_expr) = &q.where_clause {
            if !self.eval_bool(where_expr, &bindings)? {
                return Ok(QueryResult::empty());
            }
        }

        if q.select.is_some() {
            return self.project_rows(q.select.as_ref(), &[bindings]);
        }

        // Default: show context commit summary
        Ok(QueryResult {
            columns: vec!["hash".into(), "message".into(), "timestamp".into()],
            rows: vec![vec![
                Value::String(context.to_string()),
                Value::String(
                    bindings
                        .get("_")
                        .map(|b| b.commit.message.clone())
                        .unwrap_or_default(),
                ),
                Value::Int(
                    bindings
                        .get("_")
                        .map(|b| b.commit.timestamp as i64)
                        .unwrap_or(0),
                ),
            ]],
        })
    }

    fn exec_traversal_only(&mut self, q: &QueryStatement) -> ExecResultType<QueryResult> {
        let Some(trav) = &q.traversal else {
            return Ok(QueryResult::empty());
        };

        let mut bindings_list = Vec::new();

        match trav {
            TraversalClause::MergeBase(a, b) => {
                let ha = self.resolve_target_ref(a)?;
                let hb = self.resolve_target_ref(b)?;
                if let Some(h) = merge_base(self.store, ha, hb)? {
                    let c = self.store.get_commit(&h)?;
                    let bound = self.bind_commit(h, c)?;
                    let mut map = HashMap::new();
                    map.insert("hash".into(), bound.clone());
                    // Also expose as anonymous default for property access on synthetic names
                    map.insert("_".into(), bound);
                    bindings_list.push(map);
                }
            }
            TraversalClause::Ancestors(t) => {
                let start = self.resolve_target_ref(t)?;
                let ancs = epochs_core::collect_ancestors(self.store, start)?;
                let mut hashes: Vec<_> = ancs.into_iter().collect();
                hashes.sort_by_key(ToString::to_string);
                for h in hashes {
                    let c = self.store.get_commit(&h)?;
                    let bound = self.bind_commit(h, c)?;
                    let mut map = HashMap::new();
                    map.insert("hash".into(), bound);
                    bindings_list.push(map);
                }
            }
            TraversalClause::ShortestPath(a, b) => {
                let ha = self.resolve_target_ref(a)?;
                let hb = self.resolve_target_ref(b)?;
                let path = self.shortest_commit_path(ha, hb)?;
                for h in path {
                    let c = self.store.get_commit(&h)?;
                    let bound = self.bind_commit(h, c)?;
                    let mut map = HashMap::new();
                    map.insert("hash".into(), bound);
                    bindings_list.push(map);
                }
            }
        }

        if let Some(select) = &q.select {
            // Support SELECT hash, message as property-like idents on the bound commit.
            let columns: Vec<String> = select
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    item.alias
                        .clone()
                        .unwrap_or_else(|| expression_label(&item.expression, i))
                })
                .collect();
            let mut rows = Vec::new();
            for bindings in &bindings_list {
                let bound = bindings
                    .values()
                    .next()
                    .ok_or_else(|| ExecError("empty traversal binding".into()))?;
                let mut row = Vec::new();
                for item in &select.items {
                    row.push(match &item.expression {
                        Expression::Variable(name) => property_of(bound, name),
                        other => self.eval_expr(other, bindings)?,
                    });
                }
                rows.push(row);
            }
            return Ok(QueryResult { columns, rows });
        }

        // Default columns
        let mut rows = Vec::new();
        for bindings in bindings_list {
            let bound = bindings
                .values()
                .next()
                .ok_or_else(|| ExecError("empty traversal binding".into()))?;
            rows.push(vec![
                Value::String(bound.hash.to_string()),
                Value::Int(bound.commit.timestamp as i64),
                Value::String(bound.commit.message.clone()),
            ]);
        }
        Ok(QueryResult {
            columns: vec!["hash".into(), "timestamp".into(), "message".into()],
            rows,
        })
    }

    fn shortest_commit_path(&mut self, from: Hash, to: Hash) -> ExecResultType<Vec<Hash>> {
        // BFS over undirected commit graph (parents + children)
        let children = self.store.children_map()?;
        let mut queue = VecDeque::from([(from, vec![from])]);
        let mut visited = HashSet::from([from]);

        while let Some((current, path)) = queue.pop_front() {
            if current == to {
                return Ok(path);
            }
            let commit = self.store.get_commit(&current)?;
            let mut neighbors = commit.parents;
            if let Some(ch) = children.get(&current) {
                neighbors.extend(ch.iter().copied());
            }
            for n in neighbors {
                if visited.insert(n) {
                    let mut p = path.clone();
                    p.push(n);
                    queue.push_back((n, p));
                }
            }
        }
        Err(ExecError(format!("no path between {from} and {to}")))
    }

    fn exec_match(
        &mut self,
        match_clause: &MatchClause,
        context: Hash,
    ) -> ExecResultType<Vec<HashMap<String, BoundCommit>>> {
        // Universe: ancestors of context (incl. self) ∪ all reachable (for child edges)
        let mut universe = epochs_core::collect_ancestors(self.store, context)?;
        for h in self.store.reachable_commits()? {
            universe.insert(h);
        }
        let children = self.store.children_map()?;

        let mut all_rows = Vec::new();
        for pattern in &match_clause.patterns {
            let rows = self.match_pattern(pattern, &universe, &children)?;
            all_rows.extend(rows);
        }
        Ok(all_rows)
    }

    fn match_pattern(
        &mut self,
        pattern: &Pattern,
        universe: &HashSet<Hash>,
        children: &HashMap<Hash, Vec<Hash>>,
    ) -> ExecResultType<Vec<HashMap<String, BoundCommit>>> {
        let elements = &pattern.elements;
        if elements.is_empty() {
            return Ok(vec![]);
        }

        let PatternElement::Node(first) = &elements[0] else {
            return Err(ExecError("pattern must start with a node".into()));
        };

        let mut partial: Vec<HashMap<String, BoundCommit>> = Vec::new();
        for hash in universe {
            let commit = self.store.get_commit(hash)?;
            if let Some(bound) = self.try_bind_node(first, *hash, commit)? {
                let mut map = HashMap::new();
                if let Some(var) = &first.variable {
                    map.insert(var.clone(), bound);
                } else {
                    map.insert(format!("_{}", map.len()), bound);
                }
                partial.push(map);
            }
        }

        let mut i = 1;
        while i + 1 < elements.len() {
            let PatternElement::Edge(edge) = &elements[i] else {
                return Err(ExecError("expected edge in pattern".into()));
            };
            let PatternElement::Node(node) = &elements[i + 1] else {
                return Err(ExecError("expected node after edge".into()));
            };

            let mut next_partial = Vec::new();
            for bindings in partial {
                // Source = last bound node in chain (variable of previous node)
                let prev_var = pattern_node_var(&elements[i - 1], i - 1);
                let src = bindings
                    .get(&prev_var)
                    .ok_or_else(|| ExecError(format!("unbound pattern variable '{prev_var}'")))?;

                let candidates = self.expand_edge(edge, src.hash, universe, children)?;
                for cand_hash in candidates {
                    let commit = self.store.get_commit(&cand_hash)?;
                    if let Some(bound) = self.try_bind_node(node, cand_hash, commit)? {
                        let mut nb = bindings.clone();
                        let var = node
                            .variable
                            .clone()
                            .unwrap_or_else(|| format!("_{}", i + 1));
                        if let Some(existing) = nb.get(&var) {
                            if existing.hash != bound.hash {
                                continue; // variable conflict
                            }
                        }
                        nb.insert(var, bound);
                        next_partial.push(nb);
                    }
                }
            }
            partial = next_partial;
            i += 2;
        }

        Ok(partial)
    }

    fn expand_edge(
        &mut self,
        edge: &EdgePattern,
        from: Hash,
        _universe: &HashSet<Hash>,
        children: &HashMap<Hash, Vec<Hash>>,
    ) -> ExecResultType<Vec<Hash>> {
        let toward_root = match edge.edge_type {
            EdgeType::Parent | EdgeType::Ancestor => true,
            EdgeType::Child | EdgeType::Descendant => false,
        };
        let _ = edge.direction;

        let (min_hops, max_hops) = hop_bounds(edge.multiplier.as_ref(), edge.edge_type);

        let mut results = HashSet::new();
        let mut queue = VecDeque::from([(from, 0u32)]);
        let mut visited_at_depth: HashSet<(Hash, u32)> = HashSet::from([(from, 0)]);

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= min_hops && depth <= max_hops && current != from {
                results.insert(current);
            }
            if depth >= max_hops {
                continue;
            }

            let nexts: Vec<Hash> = if toward_root {
                self.store.get_commit(&current)?.parents
            } else {
                children.get(&current).cloned().unwrap_or_default()
            };

            for n in nexts {
                let nd = depth + 1;
                if visited_at_depth.insert((n, nd)) {
                    queue.push_back((n, nd));
                }
            }
        }

        Ok(results.into_iter().collect())
    }

    fn try_bind_node(
        &mut self,
        node: &NodePattern,
        hash: Hash,
        commit: Commit,
    ) -> ExecResultType<Option<BoundCommit>> {
        let label = node.label.as_deref().unwrap_or("Commit");
        if !label.eq_ignore_ascii_case("Commit") && !label.eq_ignore_ascii_case("State") {
            // Unknown labels: treat like Commit for now
        }

        let bound = self.bind_commit(hash, commit)?;

        for (key, expected) in &node.properties {
            let actual = property_of(&bound, key);
            let exp_val = eval_literal_expr(expected)?;
            if actual != exp_val {
                return Ok(None);
            }
        }

        Ok(Some(bound))
    }

    fn bind_commit(&mut self, hash: Hash, commit: Commit) -> ExecResultType<BoundCommit> {
        let entries = self.store.hamt_entries(commit.root_hamt)?;
        let mut state = BTreeMap::new();
        for (k, v) in entries {
            let key = String::from_utf8_lossy(&k).into_owned();
            state.insert(key, Value::from_bytes(&v));
        }
        Ok(BoundCommit {
            hash,
            commit,
            state,
        })
    }

    fn project_rows(
        &self,
        select: Option<&SelectClause>,
        bindings_list: &[HashMap<String, BoundCommit>],
    ) -> ExecResultType<QueryResult> {
        if bindings_list.is_empty() {
            return Ok(QueryResult::empty());
        }

        let select = match select {
            Some(s) => s,
            None => {
                // Default: all variables' hashes
                let vars: Vec<String> = bindings_list[0].keys().cloned().collect();
                let columns = vars.clone();
                let mut rows = Vec::new();
                for b in bindings_list {
                    let row = vars
                        .iter()
                        .map(|v| {
                            b.get(v)
                                .map(|c| Value::String(c.hash.to_string()))
                                .unwrap_or(Value::Null)
                        })
                        .collect();
                    rows.push(row);
                }
                return Ok(QueryResult { columns, rows });
            }
        };

        let columns: Vec<String> = select
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                item.alias
                    .clone()
                    .unwrap_or_else(|| expression_label(&item.expression, i))
            })
            .collect();

        let mut rows = Vec::new();
        for bindings in bindings_list {
            let mut row = Vec::new();
            for item in &select.items {
                row.push(self.eval_expr(&item.expression, bindings)?);
            }
            rows.push(row);
        }
        Ok(QueryResult { columns, rows })
    }

    fn eval_bool(
        &self,
        expr: &Expression,
        bindings: &HashMap<String, BoundCommit>,
    ) -> ExecResultType<bool> {
        Ok(self.eval_expr(expr, bindings)?.is_truthy())
    }

    fn eval_expr(
        &self,
        expr: &Expression,
        bindings: &HashMap<String, BoundCommit>,
    ) -> ExecResultType<Value> {
        match expr {
            Expression::Null => Ok(Value::Null),
            Expression::Bool(b) => Ok(Value::Bool(*b)),
            Expression::Int(n) => Ok(Value::Int(*n)),
            Expression::Float(n) => Ok(Value::Float(*n)),
            Expression::String(s) => Ok(Value::String(s.clone())),
            Expression::Map(m) => {
                let mut out = BTreeMap::new();
                for (k, v) in m {
                    out.insert(k.clone(), self.eval_expr(v, bindings)?);
                }
                Ok(Value::Map(out))
            }
            Expression::Variable(name) => {
                let b = bindings
                    .get(name)
                    .ok_or_else(|| ExecError(format!("unbound variable '{name}'")))?;
                Ok(Value::String(b.hash.to_string()))
            }
            Expression::PropertyAccess { object, property } => {
                if let Expression::Variable(name) = object.as_ref() {
                    let b = bindings
                        .get(name)
                        .ok_or_else(|| ExecError(format!("unbound variable '{name}'")))?;
                    return Ok(property_of(b, property));
                }
                let base = self.eval_expr(object, bindings)?;
                match base {
                    Value::Map(m) => Ok(m.get(property).cloned().unwrap_or(Value::Null)),
                    _ => Ok(Value::Null),
                }
            }
            Expression::Binary { left, op, right } => {
                let l = self.eval_expr(left, bindings)?;
                let r = self.eval_expr(right, bindings)?;
                match op {
                    BinaryOp::Eq => Ok(Value::Bool(values_eq(&l, &r))),
                    BinaryOp::Neq => Ok(Value::Bool(!values_eq(&l, &r))),
                    BinaryOp::And => Ok(Value::Bool(l.is_truthy() && r.is_truthy())),
                    BinaryOp::Or => Ok(Value::Bool(l.is_truthy() || r.is_truthy())),
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Version / mutations
    // -------------------------------------------------------------------------

    fn exec_version(&mut self, v: &VersionStatement) -> ExecResultType<MutationResult> {
        match v {
            VersionStatement::Commit(c) => self.exec_commit(c),
            VersionStatement::Branch(b) => self.exec_branch(b),
            VersionStatement::Checkout(t) => self.exec_checkout(t),
            VersionStatement::Merge(m) => self.exec_merge(m),
            VersionStatement::Diff(d) => self.exec_diff(d),
        }
    }

    fn exec_commit(&mut self, stmt: &CommitStmt) -> ExecResultType<MutationResult> {
        let head = self
            .store
            .head()?
            .ok_or_else(|| ExecError("HEAD not set".into()))?;

        let parents = if let Some(list) = &stmt.parents {
            let mut out = Vec::new();
            for t in list {
                out.push(self.resolve_target_ref(t)?);
            }
            out
        } else {
            vec![head.target]
        };

        let base_commit = self.store.get_commit(&parents[0])?;
        let base_root = if base_commit.root_hamt == Hash::ZERO {
            None
        } else {
            Some(base_commit.root_hamt)
        };

        let mut ops = Vec::new();
        for (key, expr) in &stmt.payload {
            let val = eval_literal_expr(expr)?;
            push_payload_ops(&mut ops, key, val)?;
        }

        let new_root = self.store.apply_hamt_ops(base_root, &ops)?;

        let schema =
            SchemaRegistry::load(self.store.path()).map_err(|e| ExecError(e.to_string()))?;
        let old_doc = if base_commit.root_hamt == Hash::ZERO {
            BTreeMap::new()
        } else {
            doc_from_hamt_root(self.store, base_commit.root_hamt)?
        };
        let new_doc = if new_root == Hash::ZERO {
            BTreeMap::new()
        } else {
            doc_from_hamt_root(self.store, new_root)?
        };

        let index_roots = update_indexes_for_commit(
            self.store,
            &schema,
            &old_doc,
            &new_doc,
            &base_commit.index_roots,
        )?;

        let message = stmt.message.as_deref().unwrap_or("commit");
        let new_hash = self
            .store
            .commit_with_indexes(parents, new_root, index_roots, message)?;
        self.store.update_branch(&head.name, new_hash)?;

        Ok(MutationResult {
            summary: format!("committed on branch '{}'", head.name),
            hash: Some(new_hash.to_string()),
        })
    }

    fn exec_branch(&mut self, stmt: &BranchStmt) -> ExecResultType<MutationResult> {
        match stmt {
            BranchStmt::Create { name, from } => {
                let target = match from {
                    Some(t) => self.resolve_target_ref(t)?,
                    None => self.resolve_target_ref(&TargetRef::Head)?,
                };
                self.store.create_branch(name, target)?;
                Ok(MutationResult {
                    summary: format!("created branch '{name}'"),
                    hash: Some(target.to_string()),
                })
            }
            BranchStmt::Delete { name } => {
                self.store.delete_branch(name)?;
                Ok(MutationResult {
                    summary: format!("deleted branch '{name}'"),
                    hash: None,
                })
            }
        }
    }

    fn exec_checkout(&mut self, target: &TargetRef) -> ExecResultType<MutationResult> {
        match target {
            TargetRef::Branch(name) => {
                self.store.set_head(name)?;
                let tip = self.store.get_branch(name)?.target;
                Ok(MutationResult {
                    summary: format!("checked out branch '{name}'"),
                    hash: Some(tip.to_string()),
                })
            }
            TargetRef::Head => {
                let head = self
                    .store
                    .head()?
                    .ok_or_else(|| ExecError("HEAD not set".into()))?;
                Ok(MutationResult {
                    summary: format!("already on branch '{}'", head.name),
                    hash: Some(head.target.to_string()),
                })
            }
            TargetRef::Commit(hex) => {
                let hash = self.store.resolve_hash_ref(hex)?;
                let _ = self.store.get_commit(&hash)?;
                Ok(MutationResult {
                    summary: format!("resolved commit {hash} (detached; HEAD unchanged)"),
                    hash: Some(hash.to_string()),
                })
            }
            TargetRef::Tag(name) => Err(ExecError(format!("tags not implemented: {name}"))),
        }
    }

    fn exec_merge(&mut self, stmt: &MergeStmt) -> ExecResultType<MutationResult> {
        let source = self.resolve_target_ref(&stmt.source)?;
        let into = self.resolve_target_ref(&stmt.into)?;

        let into_branch = match &stmt.into {
            TargetRef::Branch(n) => n.clone(),
            TargetRef::Head => {
                self.store
                    .head()?
                    .ok_or_else(|| ExecError("HEAD not set".into()))?
                    .name
            }
            _ => {
                return Err(ExecError("MERGE INTO requires a branch or HEAD".into()));
            }
        };

        match stmt.strategy {
            MergeStrategy::FastForward => {
                if !is_ancestor(self.store, into, source)? {
                    return Err(ExecError(
                        "fast-forward not possible: source is not a descendant of target".into(),
                    ));
                }
                self.store.update_branch(&into_branch, source)?;
                Ok(MutationResult {
                    summary: format!("fast-forwarded '{into_branch}'"),
                    hash: Some(source.to_string()),
                })
            }
            MergeStrategy::ThreeWay | MergeStrategy::Squash => Err(ExecError(
                "THREE_WAY and SQUASH merge strategies are not implemented yet".into(),
            )),
        }
    }

    fn exec_diff(&mut self, stmt: &DiffStmt) -> ExecResultType<MutationResult> {
        let left = self.resolve_target_ref(&stmt.left)?;
        let right = self.resolve_target_ref(&stmt.right)?;
        let lc = self.store.get_commit(&left)?;
        let rc = self.store.get_commit(&right)?;

        let left_entries: BTreeMap<String, Vec<u8>> = self
            .store
            .hamt_entries(lc.root_hamt)?
            .into_iter()
            .map(|(k, v)| (String::from_utf8_lossy(&k).into_owned(), v))
            .collect();
        let right_entries: BTreeMap<String, Vec<u8>> = self
            .store
            .hamt_entries(rc.root_hamt)?
            .into_iter()
            .map(|(k, v)| (String::from_utf8_lossy(&k).into_owned(), v))
            .collect();

        let mut lines = Vec::new();
        let keys: BTreeMap<_, _> = left_entries
            .keys()
            .chain(right_entries.keys())
            .map(|k| (k.clone(), ()))
            .collect();

        for key in keys.keys() {
            if let Some(path) = &stmt.path {
                if key != path && !key.starts_with(&format!("{path}.")) {
                    continue;
                }
            }
            match (left_entries.get(key), right_entries.get(key)) {
                (None, Some(r)) => lines.push(format!("+ {key} = {}", String::from_utf8_lossy(r))),
                (Some(l), None) => lines.push(format!("- {key} = {}", String::from_utf8_lossy(l))),
                (Some(l), Some(r)) if l != r => lines.push(format!(
                    "~ {key}: {} => {}",
                    String::from_utf8_lossy(l),
                    String::from_utf8_lossy(r)
                )),
                _ => {}
            }
        }

        Ok(MutationResult {
            summary: if lines.is_empty() {
                "diff: no changes".into()
            } else {
                format!("diff:\n{}", lines.join("\n"))
            },
            hash: None,
        })
    }
}

fn hop_bounds(mult: Option<&HopMultiplier>, edge_type: EdgeType) -> (u32, u32) {
    let default_max = match edge_type {
        EdgeType::Parent | EdgeType::Child => 1,
        EdgeType::Ancestor | EdgeType::Descendant => 64,
    };
    match mult {
        None => (1, default_max),
        Some(HopMultiplier::Any) => (1, 64),
        Some(HopMultiplier::Exact(n)) => (*n, *n),
        Some(HopMultiplier::Range { min, max }) => (min.unwrap_or(1), max.unwrap_or(64)),
    }
}

fn pattern_node_var(elem: &PatternElement, idx: usize) -> String {
    match elem {
        PatternElement::Node(n) => n.variable.clone().unwrap_or_else(|| format!("_{idx}")),
        _ => format!("_{idx}"),
    }
}

fn property_of(bound: &BoundCommit, key: &str) -> Value {
    match key {
        "hash" => Value::String(bound.hash.to_string()),
        "message" => Value::String(bound.commit.message.clone()),
        "timestamp" => Value::Int(bound.commit.timestamp as i64),
        "root_hamt" => Value::String(bound.commit.root_hamt.to_string()),
        other => bound.state.get(other).cloned().unwrap_or(Value::Null),
    }
}

fn eval_literal_expr(expr: &Expression) -> ExecResultType<Value> {
    match expr {
        Expression::Null => Ok(Value::Null),
        Expression::Bool(b) => Ok(Value::Bool(*b)),
        Expression::Int(n) => Ok(Value::Int(*n)),
        Expression::Float(n) => Ok(Value::Float(*n)),
        Expression::String(s) => Ok(Value::String(s.clone())),
        Expression::Map(m) => {
            let mut out = BTreeMap::new();
            for (k, v) in m {
                out.insert(k.clone(), eval_literal_expr(v)?);
            }
            Ok(Value::Map(out))
        }
        other => Err(ExecError(format!(
            "expression not allowed as literal payload: {other:?}"
        ))),
    }
}

/// Flatten nested maps in a COMMIT payload into dotted HAMT keys.
fn push_payload_ops(ops: &mut Vec<HamtOp>, prefix: &str, val: Value) -> ExecResultType<()> {
    match val {
        Value::Map(m) => {
            for (k, v) in m {
                let path = if prefix.is_empty() {
                    k
                } else {
                    format!("{prefix}.{k}")
                };
                push_payload_ops(ops, &path, v)?;
            }
            Ok(())
        }
        other => {
            ops.push(HamtOp::Put {
                key: prefix.as_bytes().to_vec(),
                value: other.to_storage_bytes().map_err(ExecError)?,
            });
            Ok(())
        }
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        (Value::String(x), Value::String(y)) => {
            if x == y {
                return true;
            }
            // Allow commit-hash prefix match (short hex on either side).
            let xl = x.to_ascii_lowercase();
            let yl = y.to_ascii_lowercase();
            (xl.len() >= 6 && yl.starts_with(&xl)) || (yl.len() >= 6 && xl.starts_with(&yl))
        }
        (Value::Int(x), Value::String(y)) | (Value::String(y), Value::Int(x)) => {
            y.parse::<i64>().ok() == Some(*x)
        }
        _ => false,
    }
}

fn expression_label(expr: &Expression, idx: usize) -> String {
    match expr {
        Expression::Variable(n) => n.clone(),
        Expression::PropertyAccess { property, .. } => property.clone(),
        _ => format!("col{idx}"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::exec::Value;
    use crate::schema::{lookup_index, migrate};
    use std::env;
    use std::fs;

    #[test]
    fn migrate_and_commit_populate_index_roots() {
        let dir = env::temp_dir().join(format!("epochs_idx_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let (mut store, _) = DiskStore::init(&dir, "main", "genesis").unwrap();
        fs::create_dir_all(dir.join("migrations")).unwrap();
        fs::write(
            dir.join("migrations/001_init.eql"),
            r#"
            CREATE COLLECTION items KEY id STRING;
            CREATE INDEX ON items (id);
            CREATE INDEX ON items PATH "meta.prefs.theme" TYPE STRING;
            "#,
        )
        .unwrap();

        let report = migrate(&dir).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert_eq!(report.schema.all_indexes().len(), 2);

        let mut engine = Engine::new(&mut store);
        let results = engine
            .execute(
                r#"
                COMMIT {
                    id: "item-1",
                    meta: { prefs: { theme: "dark" } }
                } MESSAGE "seed";
                "#,
            )
            .unwrap();
        let hash_hex = match &results[0] {
            ExecResult::Mutation(m) => m.hash.clone().expect("commit hash"),
            other => panic!("expected mutation, got {other:?}"),
        };

        let hash = store.resolve_hash_ref(&hash_hex).unwrap();
        let commit = store.get_commit(&hash).unwrap();
        assert!(commit.index_roots.contains_key("items.by_id"));
        assert!(commit.index_roots.contains_key("items.by_meta_prefs_theme"));

        let theme_idx = report
            .schema
            .all_indexes()
            .into_iter()
            .find(|i| i.path == "meta.prefs.theme")
            .unwrap();
        let pk = lookup_index(
            &mut store,
            &commit.index_roots,
            theme_idx,
            &Value::String("dark".into()),
        )
        .unwrap()
        .expect("theme index hit");
        assert_eq!(pk, b"item-1");

        fs::remove_dir_all(&dir).ok();
    }
}
