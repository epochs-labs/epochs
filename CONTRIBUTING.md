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
5. Pull requests use [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md).
   CI runs fmt/clippy/tests **and** coverage (`cargo llvm-cov` → Codecov). Merging
   should not lower project coverage materially; new lines should be covered
   (see [`codecov.yml`](codecov.yml)).
6. Do not commit secrets, local `target/`, or `benches/out/` scratch files.
   Published bench charts/CSV under `benches/charts/` and `benches/RESULTS.md`
   are fine when regenerating intentionally.

### Coverage locally

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
cargo llvm-cov --workspace --exclude epochs-bench --html --open
```

### GitHub / Codecov setup (maintainers)

1. Install the [Codecov GitHub App](https://github.com/apps/codecov) on `epochs-labs`.
2. (Recommended) Add repo secret `CODECOV_TOKEN` from the Codecov project settings
   so PR uploads from forks are reliable.
3. In branch protection for `main`, require checks:
   - `fmt · clippy · test`
   - Codecov / `coverage` statuses once they appear on a PR

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
