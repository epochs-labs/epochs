<p align="center">
  <img src="assets/logo.svg" alt="epochs" width="160" height="160" />
</p>

<h1 align="center">epochs</h1>

<p align="center">
  <em>Merkle-DAG database — branch, merge, and time-travel through data. Fast to embed, ready for history.</em>
</p>

<p align="center">
  <a href="https://github.com/epochs-labs/epochs/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/epochs-labs/epochs/ci.yml?branch=main&style=flat-square&label=CI" alt="CI" /></a>
  <a href="https://github.com/epochs-labs/epochs/blob/main/.github/coverage-floor"><img src="https://img.shields.io/badge/coverage-%E2%89%A568%25-brightgreen?style=flat-square&label=coverage" alt="Coverage floor" /></a>
  <a href="https://github.com/epochs-labs/epochs/releases"><img src="https://img.shields.io/github/v/release/epochs-labs/epochs?style=flat-square&label=release" alt="Release" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-edition%202021-orange?style=flat-square" alt="Rust edition 2021" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square" alt="License" /></a>
  <a href="https://github.com/epochs-labs/epochs/stargazers"><img src="https://img.shields.io/github/stars/epochs-labs/epochs?style=flat-square" alt="Stars" /></a>
</p>

> **Status: early public 0.1.x.** Useful as a local engine and query language.
> APIs and EpochQL may break without a major version bump until **1.0**.
> Dual-licensed **MIT OR Apache-2.0** ([`LICENSE-MIT`](LICENSE-MIT),
> [`LICENSE-APACHE`](LICENSE-APACHE)).

## Vision

**epochs** is generic storage infrastructure. Domain products (e.g. agent
platforms) sit above it:

```
product SDK / UI          →  domain types (agents, threads, …)
epochs-server (EPX)       →  auth, workspaces, multi-tenant
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
| [`epochs-server`](epochs-server) | Epoch Protocol (EPX) TCP server (`epochs://host:7420`) |
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
| Epoch Protocol server (EPX TCP) | Done (single-writer) |
| Tags (`TAG …`) | **Not implemented** (parsed / rejected) |
| `MERGE … STRATEGY THREE_WAY` / `SQUASH` | **Not implemented** (FF only) |
| Proly trees (CDC / range) | Design only — [`epochs-core/docs/proly-v1.md`](epochs-core/docs/proly-v1.md) |
| Multi-writer / network sync | **Future** |
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
