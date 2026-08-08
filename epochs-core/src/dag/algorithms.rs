//! Graph algorithms over commit DAGs.

use std::collections::{HashSet, VecDeque};

use crate::error::{EpochsError, Result};
use crate::hash::Hash;
use crate::store::DagStore;

/// Returns the nearest common ancestor of two commits, if one exists.
pub fn merge_base(store: &mut impl DagStore, a: Hash, b: Hash) -> Result<Option<Hash>> {
    if a == b {
        return Ok(Some(a));
    }

    let ancestors_a = collect_ancestors(store, a)?;
    let mut queue = VecDeque::from([b]);
    let mut visited = HashSet::from([b]);

    while let Some(current) = queue.pop_front() {
        if ancestors_a.contains(&current) {
            return Ok(Some(current));
        }

        let commit = store.get_commit(&current)?;
        for parent in commit.parents {
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    Ok(None)
}

/// Collect all ancestor commit hashes of `start`, including `start` itself.
pub fn collect_ancestors(store: &mut impl DagStore, start: Hash) -> Result<HashSet<Hash>> {
    let mut ancestors = HashSet::new();
    let mut queue = VecDeque::from([start]);

    while let Some(current) = queue.pop_front() {
        if !ancestors.insert(current) {
            continue;
        }

        let commit = store.get_commit(&current)?;
        for parent in commit.parents {
            queue.push_back(parent);
        }
    }

    Ok(ancestors)
}

/// Returns true if `ancestor` is an ancestor of (or equal to) `descendant`.
pub fn is_ancestor(store: &mut impl DagStore, ancestor: Hash, descendant: Hash) -> Result<bool> {
    if ancestor == descendant {
        return Ok(true);
    }

    let ancestors = collect_ancestors(store, descendant)?;
    Ok(ancestors.contains(&ancestor))
}

/// Walk from `start` toward parents up to `max_depth` generations.
pub fn ancestors_within_depth(
    store: &mut impl DagStore,
    start: Hash,
    max_depth: usize,
) -> Result<Vec<Hash>> {
    if max_depth == 0 {
        return Ok(vec![]);
    }

    let mut result = Vec::new();
    let mut queue = VecDeque::from([(start, 0usize)]);
    let mut visited = HashSet::from([start]);

    while let Some((current, depth)) = queue.pop_front() {
        if depth > 0 {
            result.push(current);
        }
        if depth >= max_depth {
            continue;
        }

        let commit = store
            .get_commit(&current)
            .map_err(|_| EpochsError::CommitNotFound(current))?;
        for parent in commit.parents {
            if visited.insert(parent) {
                queue.push_back((parent, depth + 1));
            }
        }
    }

    Ok(result)
}
