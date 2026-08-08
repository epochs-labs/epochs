//! Secondary path-index maintenance on commit.

use std::collections::BTreeMap;

use epochs_core::{DiskStore, Hash};

use crate::exec::Value;
use crate::schema::path::{extract_path, flatten_entries, value_to_index_key};
use crate::schema::registry::{IndexDef, SchemaRegistry};

/// Build the canonical index root name: `collection.by_<path_with_underscores>`.
pub fn index_name_for(collection: &str, path: &str) -> String {
    let slug = path.replace('.', "_");
    format!("{collection}.by_{slug}")
}

/// Update all schema indexes for a new document state.
///
/// - `old_doc` / `new_doc`: flat string maps of the previous and next HAMT state
/// - `prev_index_roots`: inherited from the parent commit
/// - Returns the new `index_roots` map for the child commit
pub fn update_indexes_for_commit(
    store: &mut DiskStore,
    schema: &SchemaRegistry,
    old_doc: &BTreeMap<String, Value>,
    new_doc: &BTreeMap<String, Value>,
    prev_index_roots: &BTreeMap<String, Hash>,
) -> Result<BTreeMap<String, Hash>, String> {
    let mut index_roots = prev_index_roots.clone();

    if schema.collections.is_empty() {
        return Ok(index_roots);
    }

    for coll in schema.collections.values() {
        let pk = primary_key_bytes(coll, new_doc).or_else(|| primary_key_bytes(coll, old_doc));

        for idx in &coll.indexes {
            let name = idx.name.clone();
            let prev_root = index_roots.get(&name).copied().filter(|h| *h != Hash::ZERO);

            let old_val = extract_path(old_doc, &idx.path);
            let new_val = extract_path(new_doc, &idx.path);

            // Remove stale mapping when value changed or document removed key
            if let (Some(old_v), Some(_)) = (&old_val, &pk) {
                if old_val != new_val {
                    if let Some(old_key) = value_to_index_key(old_v) {
                        let next = store
                            .index_remove(prev_root, &old_key)
                            .map_err(|e| e.to_string())?;
                        match next {
                            Some(h) => {
                                index_roots.insert(name.clone(), h);
                            }
                            None => {
                                index_roots.remove(&name);
                            }
                        }
                    }
                }
            }

            let cur_root = index_roots.get(&name).copied().filter(|h| *h != Hash::ZERO);

            if let (Some(new_v), Some(pk_bytes)) = (&new_val, &pk) {
                if let Some(new_key) = value_to_index_key(new_v) {
                    let h = store
                        .index_put(cur_root, &new_key, pk_bytes)
                        .map_err(|e| e.to_string())?;
                    index_roots.insert(name, h);
                }
            }
        }
    }

    // Drop index roots for indexes no longer in schema
    let live: BTreeMap<_, _> = schema
        .all_indexes()
        .into_iter()
        .map(|i| i.name.clone())
        .map(|n| (n.clone(), ()))
        .collect();
    index_roots.retain(|k, _| live.contains_key(k));

    Ok(index_roots)
}

fn primary_key_bytes(
    coll: &crate::schema::registry::CollectionSchema,
    doc: &BTreeMap<String, Value>,
) -> Option<Vec<u8>> {
    if coll.primary_key.is_empty() {
        // Fall back: use first available "id" or entire doc fingerprint
        if let Some(v) = extract_path(doc, "id") {
            return value_to_index_key(&v);
        }
        return None;
    }
    let mut parts = Vec::new();
    for field in &coll.primary_key {
        let v = extract_path(doc, &field.path)?;
        let bytes = value_to_index_key(&v)?;
        parts.push(bytes);
    }
    if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        // Composite: join with 0x1f unit separator
        let mut out = Vec::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                out.push(0x1f);
            }
            out.extend_from_slice(p);
        }
        Some(out)
    }
}

/// Look up primary-key bytes for an indexed field value.
pub fn lookup_index(
    store: &mut DiskStore,
    index_roots: &BTreeMap<String, Hash>,
    index: &IndexDef,
    value: &Value,
) -> Result<Option<Vec<u8>>, String> {
    let root = match index_roots.get(&index.name) {
        Some(h) if *h != Hash::ZERO => *h,
        _ => return Ok(None),
    };
    let key = match value_to_index_key(value) {
        Some(k) => k,
        None => return Ok(None),
    };
    store.index_get(root, &key).map_err(|e| e.to_string())
}

/// Helper: build doc map from a HAMT root.
pub fn doc_from_hamt_root(
    store: &mut DiskStore,
    root: Hash,
) -> Result<BTreeMap<String, Value>, String> {
    let entries = store.hamt_entries(root).map_err(|e| e.to_string())?;
    Ok(flatten_entries(&entries))
}
