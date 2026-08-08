# epochs

A version-controlled Merkle-DAG database engine — git-like storage for mutable
state. Branch, merge, and time-travel through data. Written in Rust.

> **Status: early public 0.1.x.** Useful as a local engine and query language.
> APIs and EpochQL may break without a major version bump until **1.0**.
> Dual-licensed **MIT OR Apache-2.0** ([`LICENSE-MIT`](LICENSE-MIT),
> [`LICENSE-APACHE`](LICENSE-APACHE)).

## Vision

**epochs** is generic storage infrastructure. Domain products (e.g. agent
platforms) sit above it:

```
product SDK / UI          →  domain types (agents, threads, …)
epochs-server (future)    →  auth, workspaces, multi-tenant
epochs-core + epochql     →  collections, HAMT docs, DAG commits, path indexes
```

Core has **zero domain logic**. An `AgentId` is just a primary key in a collection schema.

## Architecture

```
EpochQL (lexer → parser → executor)
     │
Persistent HAMT + optional index_roots on each Commit
     │
Custom binary codecs (little-endian)
     │
Append-only CAS (.epl / .epi) — std::fs + blake3
```

## Crates

| Crate | Purpose |
|-------|---------|
| [`epochs-core`](epochs-core) | CAS, HAMT, commits (`index_roots`), `DiskStore` |
| [`epochql`](epochql) | Lexer, parser, executor, **`.eql` migrations + path indexes** |
| [`epochs-cli`](epochs-cli) | `init`, `commit`, `branch`, `checkout`, `migrate`, `query` |
| [`epochs-bench`](epochs-bench) | Fair Docker deep-history benches — [`benches/`](benches/) |

## What works today

| Feature | Status |
|---------|--------|
| CAS + persistent HAMT + `DiskStore` | Done |
| Commit v2 + `index_roots` | Done |
| Branches, checkout, fast-forward merge | Done |
| EpochQL parser + executor (schema-aware `COMMIT`) | Done |
| `.eql` migrations + path indexes | Done |
| CLI (`epochs`) | Done |
| Fair Docker benchmarks (deep history) | Done |
| Tags (`TAG …`) | **Not implemented** (parsed / rejected) |
| `MERGE … STRATEGY THREE_WAY` / `SQUASH` | **Not implemented** (FF only) |
| Proly trees (CDC / range) | Design only — [`epochs-core/docs/proly-v1.md`](epochs-core/docs/proly-v1.md) |
| Multi-writer / network sync / server | **Future** |
| Pack / GC / io_uring | **Future** |

See [CONTRIBUTING.md](CONTRIBUTING.md) to hack on the project.

## Schema migrations (`.eql`)

```
.epochs/migrations/
  001_init.eql
  002_add_theme_index.eql
```

```eql
CREATE COLLECTION items KEY id STRING;
CREATE INDEX ON items (id);
CREATE INDEX ON items PATH "meta.prefs.theme" TYPE STRING;
```

See [`epochql/docs/MIGRATIONS.md`](epochql/docs/MIGRATIONS.md). Run `epochs migrate` to apply.

## Quick start

```bash
cargo build --workspace
cargo test --workspace

cargo run -p epochs-cli -- init
cargo run -p epochs-cli -- migrate .
cargo run -p epochs-cli -- query 'CREATE BRANCH experiment FROM HEAD; COMMIT { status: "running" } MESSAGE "go";'
cargo run -p epochs-cli -- query 'MATCH (c:Commit) WHERE c.status = "running" SELECT c.hash, c.message;'
```

## EpochQL (subset)

```sql
USE BRANCH experiment MATCH (c:Commit) SELECT c.status;
TRAVERSE MERGE_BASE(BRANCH a, BRANCH b);
DIFF BRANCH plan_alpha AND BRANCH plan_beta;
MERGE BRANCH plan_alpha INTO BRANCH main STRATEGY FAST_FORWARD;
```

Language reference: [`epochql/docs/LANGUAGE.md`](epochql/docs/LANGUAGE.md).

## Benchmarks

Deep history (fixed live keys × many commits), equal Docker cgroups:

```bash
./benches/run.sh smoke
./benches/run-ladder.sh --quick
```

Details and charts: [`benches/README.md`](benches/README.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
