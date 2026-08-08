//! Path extraction from flat / nested document maps.

use std::collections::BTreeMap;

use crate::exec::Value;

/// Build a string-keyed map from HAMT byte entries (utf-8 lossy keys/values).
pub fn flatten_entries(entries: &[(Vec<u8>, Vec<u8>)]) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for (k, v) in entries {
        let key = String::from_utf8_lossy(k).into_owned();
        map.insert(key, Value::from_bytes(v));
    }
    map
}

/// Extract a value at a dotted path from a flat or nested map.
///
/// Resolution order:
/// 1. Exact key match for the full path (`"meta.prefs.theme"`)
/// 2. Nested walk: `meta` → map → `prefs` → map → `theme`
pub fn extract_path(doc: &BTreeMap<String, Value>, path: &str) -> Option<Value> {
    if let Some(v) = doc.get(path) {
        return Some(v.clone());
    }

    let mut parts = path.split('.').filter(|p| !p.is_empty());
    let first = parts.next()?;
    let mut current = doc.get(first)?.clone();
    for part in parts {
        match current {
            Value::Map(m) => {
                current = m.get(part)?.clone();
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Encode a runtime value as index key bytes.
pub fn value_to_index_key(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(if *b {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        }),
        Value::Int(n) => Some(n.to_string().into_bytes()),
        Value::Float(n) => Some(n.to_string().into_bytes()),
        Value::String(s) => Some(s.as_bytes().to_vec()),
        Value::Map(_) => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn exact_and_nested_paths() {
        let mut doc = BTreeMap::new();
        doc.insert("id".into(), Value::String("a1".into()));
        doc.insert("meta.prefs.theme".into(), Value::String("dark".into()));

        assert_eq!(extract_path(&doc, "id"), Some(Value::String("a1".into())));
        assert_eq!(
            extract_path(&doc, "meta.prefs.theme"),
            Some(Value::String("dark".into()))
        );

        let mut nested = BTreeMap::new();
        let mut prefs = BTreeMap::new();
        prefs.insert("theme".into(), Value::String("light".into()));
        nested.insert("prefs".into(), Value::Map(prefs));
        let mut doc2 = BTreeMap::new();
        doc2.insert("meta".into(), Value::Map(nested));
        assert_eq!(
            extract_path(&doc2, "meta.prefs.theme"),
            Some(Value::String("light".into()))
        );
    }
}
