# Contributing to epochs

Thanks for your interest. This project is an early **0.1.x** Merkle-DAG database
engine — expect breaking API changes until 1.0.

## Before you open a PR

1. Read the [README](README.md) status table (what works vs roadmap).
2. Run locally:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

3. Keep diffs focused. Prefer small PRs over large refactors.
4. **Tests required** for new or changed behavior (`cargo test --workspace`
   must cover it — unit and/or integration). Do not land features without tests.
5. Do not commit secrets, local `target/`, or `benches/out/` scratch files.
   Published bench charts/CSV under `benches/charts/` and `benches/RESULTS.md`
   are fine when regenerating intentionally.

## Scope

| Area | Notes |
|------|--------|
| `epochs-core` | Storage engine (CAS, HAMT, commits) — keep **domain-free** |
| `epochql` | Query language + migrations |
| `epochs-cli` | Developer CLI |
| `epochs-server` | Epoch Protocol (EPX) TCP server |
| `epochs-bench` | Fair Docker benchmarks only |

Agent product / SaaS concerns belong **outside** this repo unless they are thin
examples. Server protocol and auth-shaped plumbing may live in `epochs-server`.


## License

By contributing, you agree your contributions are dual-licensed under
**MIT OR Apache-2.0**, the same as the rest of the project.
