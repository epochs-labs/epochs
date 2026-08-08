//! Schema types persisted in `.epochs/schema.state`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::error::{ParseError, Result};
use crate::schema::ddl::FieldType;

/// A primary-key or indexed field definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDef {
    /// Dotted path (e.g. `id` or `meta.prefs.theme`).
    pub path: String,
    /// Declared type.
    pub field_type: FieldType,
}

/// A secondary (or primary-key) index on a collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexDef {
    /// Stable index name stored in `Commit.index_roots` (e.g. `items.by_id`).
    pub name: String,
    /// Field path being indexed.
    pub path: String,
    /// Value type.
    pub field_type: FieldType,
}

/// One collection in the schema registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionSchema {
    /// Collection name.
    pub name: String,
    /// Primary key fields (composite supported).
    pub primary_key: Vec<FieldDef>,
    /// Indexes (including optional explicit PK index).
    pub indexes: Vec<IndexDef>,
}

/// Full schema registry for a repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaRegistry {
    /// Collections by name.
    pub collections: BTreeMap<String, CollectionSchema>,
}

impl SchemaRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from `.epochs/schema.state`, or empty if missing.
    pub fn load(repo_path: &Path) -> Result<Self> {
        let path = repo_path.join("schema.state");
        if !path.exists() {
            return Ok(Self::new());
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| ParseError::new(format!("read schema.state: {e}")))?;
        Self::parse(&text)
    }

    /// Persist to `.epochs/schema.state`.
    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let path = repo_path.join("schema.state");
        fs::write(&path, self.to_state_file())
            .map_err(|e| ParseError::new(format!("write schema.state: {e}")))?;
        Ok(())
    }

    /// Parse the line-oriented schema.state format.
    pub fn parse(text: &str) -> Result<Self> {
        let mut registry = Self::new();
        let mut current: Option<String> = None;

        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err =
                |msg: String| ParseError::at(0, format!("schema.state line {}: {msg}", lineno + 1));

            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.first().copied() {
                Some("COLLECTION") => {
                    let name = parts
                        .get(1)
                        .ok_or_else(|| err("expected collection name".into()))?
                        .to_string();
                    registry.collections.insert(
                        name.clone(),
                        CollectionSchema {
                            name: name.clone(),
                            primary_key: Vec::new(),
                            indexes: Vec::new(),
                        },
                    );
                    current = Some(name);
                }
                Some("KEY") => {
                    let coll = current
                        .as_ref()
                        .ok_or_else(|| err("KEY outside COLLECTION".into()))?;
                    let path = parts
                        .get(1)
                        .ok_or_else(|| err("expected KEY path".into()))?
                        .to_string();
                    let ty = parts
                        .get(2)
                        .ok_or_else(|| err("expected KEY type".into()))?;
                    let field_type =
                        FieldType::parse(ty).ok_or_else(|| err(format!("unknown type '{ty}'")))?;
                    registry
                        .collections
                        .get_mut(coll)
                        .ok_or_else(|| err("missing collection".into()))?
                        .primary_key
                        .push(FieldDef { path, field_type });
                }
                Some("INDEX") => {
                    let name = parts
                        .get(1)
                        .ok_or_else(|| err("expected INDEX name".into()))?
                        .to_string();
                    if parts.get(2) != Some(&"PATH") {
                        return Err(err("expected PATH after index name".into()));
                    }
                    let path = parts
                        .get(3)
                        .ok_or_else(|| err("expected index path".into()))?
                        .to_string();
                    if parts.get(4) != Some(&"TYPE") {
                        return Err(err("expected TYPE after path".into()));
                    }
                    let ty = parts.get(5).ok_or_else(|| err("expected type".into()))?;
                    let field_type =
                        FieldType::parse(ty).ok_or_else(|| err(format!("unknown type '{ty}'")))?;
                    let coll = name
                        .split('.')
                        .next()
                        .ok_or_else(|| err("invalid index name".into()))?
                        .to_string();
                    let entry =
                        registry
                            .collections
                            .entry(coll.clone())
                            .or_insert(CollectionSchema {
                                name: coll,
                                primary_key: Vec::new(),
                                indexes: Vec::new(),
                            });
                    entry.indexes.push(IndexDef {
                        name,
                        path,
                        field_type,
                    });
                }
                Some(other) => {
                    return Err(err(format!("unknown directive '{other}'")));
                }
                None => {}
            }
        }

        Ok(registry)
    }

    /// Serialize to schema.state text.
    pub fn to_state_file(&self) -> String {
        let mut out = String::from("# epochs schema.state — generated; prefer .eql migrations\n");
        for coll in self.collections.values() {
            out.push_str(&format!("COLLECTION {}\n", coll.name));
            for key in &coll.primary_key {
                out.push_str(&format!("KEY {} {}\n", key.path, key.field_type));
            }
            for idx in &coll.indexes {
                out.push_str(&format!(
                    "INDEX {} PATH {} TYPE {}\n",
                    idx.name, idx.path, idx.field_type
                ));
            }
        }
        out
    }

    /// All indexes across collections.
    pub fn all_indexes(&self) -> Vec<&IndexDef> {
        self.collections
            .values()
            .flat_map(|c| c.indexes.iter())
            .collect()
    }
}

impl fmt::Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String => write!(f, "STRING"),
            Self::Int => write!(f, "INT"),
            Self::Bool => write!(f, "BOOL"),
            Self::Bytes => write!(f, "BYTES"),
        }
    }
}
