# EpochQL Schema Migrations (`.eql`)

Schema and indexes are **optional**. Without them, epochs is a schemaless
versioned document DAG. With them, you get declared collections, primary keys,
and path indexes — still domain-agnostic (no “AgentId” in core).

## Layout (SQL-migration style)

```
.epochs/
├── data/…
├── migrations/
│   ├── 001_init.eql
│   ├── 002_add_theme_index.eql
│   └── 003_add_owner_index.eql
├── schema.lock          # applied migration filenames
└── schema.state         # generated registry (collections / keys / indexes)
```

Migrations are plain text **`.eql`** files, applied in lexicographic order.
Each file is append-only history: **never edit an applied migration**; add a new
file to create an index or collection (same habit as Flyway / diesel / Prisma).

Apply with:

```bash
epochs migrate .
# or: epochs migrate /path/to/project
```

Looks for `<project>/.epochs/migrations/*.eql` (and writes `schema.lock` / `schema.state` under `.epochs`).

## Language (DDL subset)

```eql
-- 001_init.eql
-- Create a generic collection with a string primary key.
CREATE COLLECTION items
  KEY id STRING;

-- Primary-key index (equality lookup by id)
CREATE INDEX ON items (id);

-- 002_add_theme_index.eql
-- Secondary index on a dotted sub-field path
CREATE INDEX ON items PATH "meta.prefs.theme" TYPE STRING;

-- Agent-shaped data is just another collection:
-- CREATE COLLECTION agent_memory KEY agent_id STRING;
-- CREATE INDEX ON agent_memory (agent_id);
-- CREATE INDEX ON agent_memory PATH "memory.prefs.theme" TYPE STRING;
```

### Grammar (DDL)

```
MigrationFile     ::= Statement* ;
Statement         ::= CreateCollection | CreateIndex | DropIndex | ";" ;
CreateCollection  ::= "CREATE" "COLLECTION" Ident
                      "KEY" Path Type ( "," Path Type )* ";" ;
CreateIndex       ::= "CREATE" "INDEX" "ON" Ident
                      ( "(" Path ")" | "PATH" StringLiteral "TYPE" Type )
                      ";" ;
DropIndex         ::= "DROP" "INDEX" "ON" Ident
                      ( "(" Path ")" | "PATH" StringLiteral ) ";" ;
Path              ::= Ident ( "." Ident )* ;
Type              ::= "STRING" | "INT" | "BOOL" | "BYTES" ;
```

## How indexes attach to commits

Each commit has:

```text
index_roots: BTreeMap<String, Hash>   // e.g. "items.by_id" → HAMT root
```

On `COMMIT`, the executor:

1. Loads `schema.state` (if present)
2. Diffs old vs new document maps (path extraction on flat dotted keys or nested maps)
3. Path-copies the relevant index HAMTs and writes updated `index_roots`

Branching stays O(1): data root and index roots fork together.

Index names: `{collection}.by_{path_with_underscores}`  
(e.g. `meta.prefs.theme` → `items.by_meta_prefs_theme`).

## Status

| Piece | Status |
|-------|--------|
| `.eql` format + docs | Done |
| `Commit.index_roots` | Done (v2 commit codec) |
| Migration runner (`schema.lock` / `schema.state`) | Done |
| Path extraction + index updates on COMMIT | Done |
| `epochs migrate` CLI | Done |
