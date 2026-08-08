//! Schema registry, `.eql` migrations, and path indexes.
//!
//! Collections and indexes are optional. When present, commits populate
//! [`epochs_core::Commit::index_roots`] for declared paths.

mod ddl;
mod index;
mod migrate;
mod path;
mod registry;

pub use ddl::{parse_migration, DdlStatement, FieldType};
pub use index::{doc_from_hamt_root, index_name_for, lookup_index, update_indexes_for_commit};
pub use migrate::{migrate, MigrationReport};
pub use path::{extract_path, flatten_entries};
pub use registry::{CollectionSchema, FieldDef, IndexDef, SchemaRegistry};
