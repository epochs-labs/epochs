//! Migration runner for `.epochs/migrations/*.eql`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ParseError, Result};
use crate::schema::ddl::{parse_migration, DdlStatement};
use crate::schema::index::index_name_for;
use crate::schema::registry::{CollectionSchema, FieldDef, IndexDef, SchemaRegistry};

/// Result of applying pending migrations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    /// Migration file names that were applied in this run.
    pub applied: Vec<String>,
    /// Migration file names already recorded in schema.lock.
    pub skipped: Vec<String>,
    /// Resulting schema after apply.
    pub schema: SchemaRegistry,
}

/// Apply all pending `.eql` files under `repo/migrations/` in sorted order.
///
/// Updates `schema.lock` and `schema.state`. Idempotent: already-applied files
/// are skipped.
pub fn migrate(repo_path: &Path) -> Result<MigrationReport> {
    let migrations_dir = repo_path.join("migrations");
    fs::create_dir_all(&migrations_dir)
        .map_err(|e| ParseError::new(format!("create migrations dir: {e}")))?;

    let mut applied_lock = read_lock(repo_path)?;
    let mut schema = SchemaRegistry::load(repo_path)?;
    let files = list_migration_files(&migrations_dir)?;

    let mut newly_applied = Vec::new();
    let mut skipped = Vec::new();

    for file in files {
        let name = file
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if applied_lock.contains(&name) {
            skipped.push(name);
            continue;
        }

        let source = fs::read_to_string(&file)
            .map_err(|e| ParseError::new(format!("read migration {name}: {e}")))?;
        let stmts = parse_migration(&source)?;
        apply_statements(&mut schema, &stmts)?;
        applied_lock.push(name.clone());
        newly_applied.push(name);
    }

    write_lock(repo_path, &applied_lock)?;
    schema.save(repo_path)?;

    Ok(MigrationReport {
        applied: newly_applied,
        skipped,
        schema,
    })
}

fn list_migration_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(dir).map_err(|e| ParseError::new(format!("read migrations: {e}")))? {
        let entry = entry.map_err(|e| ParseError::new(format!("read migrations: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("eql") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn read_lock(repo_path: &Path) -> Result<Vec<String>> {
    let path = repo_path.join("schema.lock");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).map_err(|e| ParseError::new(format!("read schema.lock: {e}")))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

fn write_lock(repo_path: &Path, applied: &[String]) -> Result<()> {
    let path = repo_path.join("schema.lock");
    let mut body = String::from("# epochs schema.lock — applied migrations (do not edit)\n");
    for name in applied {
        body.push_str(name);
        body.push('\n');
    }
    fs::write(path, body).map_err(|e| ParseError::new(format!("write schema.lock: {e}")))?;
    Ok(())
}

fn apply_statements(schema: &mut SchemaRegistry, stmts: &[DdlStatement]) -> Result<()> {
    for stmt in stmts {
        match stmt {
            DdlStatement::CreateCollection { name, keys } => {
                if schema.collections.contains_key(name) {
                    return Err(ParseError::new(format!(
                        "collection '{name}' already exists"
                    )));
                }
                let primary_key = keys
                    .iter()
                    .map(|(path, ty)| FieldDef {
                        path: path.clone(),
                        field_type: *ty,
                    })
                    .collect();
                schema.collections.insert(
                    name.clone(),
                    CollectionSchema {
                        name: name.clone(),
                        primary_key,
                        indexes: Vec::new(),
                    },
                );
            }
            DdlStatement::CreateIndex {
                collection,
                path,
                field_type,
            } => {
                let coll = schema
                    .collections
                    .get_mut(collection)
                    .ok_or_else(|| ParseError::new(format!("unknown collection '{collection}'")))?;
                let name = index_name_for(collection, path);
                if coll.indexes.iter().any(|i| i.name == name) {
                    return Err(ParseError::new(format!("index '{name}' already exists")));
                }
                coll.indexes.push(IndexDef {
                    name,
                    path: path.clone(),
                    field_type: *field_type,
                });
            }
            DdlStatement::DropIndex { collection, path } => {
                let coll = schema
                    .collections
                    .get_mut(collection)
                    .ok_or_else(|| ParseError::new(format!("unknown collection '{collection}'")))?;
                let name = index_name_for(collection, path);
                let before = coll.indexes.len();
                coll.indexes.retain(|i| i.name != name && i.path != *path);
                if coll.indexes.len() == before {
                    return Err(ParseError::new(format!(
                        "index on '{collection}.{path}' not found"
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::env;

    #[test]
    fn applies_migrations_idempotently() {
        let dir = env::temp_dir().join("epochs_migrate_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("migrations")).unwrap();
        fs::write(
            dir.join("migrations/001_init.eql"),
            r#"
            CREATE COLLECTION items KEY id STRING;
            CREATE INDEX ON items (id);
            "#,
        )
        .unwrap();
        fs::write(
            dir.join("migrations/002_theme.eql"),
            r#"
            CREATE INDEX ON items PATH "meta.prefs.theme" TYPE STRING;
            "#,
        )
        .unwrap();

        let report = migrate(&dir).unwrap();
        assert_eq!(report.applied.len(), 2);
        assert!(report.schema.collections.contains_key("items"));
        assert_eq!(report.schema.collections["items"].indexes.len(), 2);

        let report2 = migrate(&dir).unwrap();
        assert!(report2.applied.is_empty());
        assert_eq!(report2.skipped.len(), 2);

        fs::remove_dir_all(&dir).ok();
    }
}
