# Contributing to epochs

Thanks for your interest. This project is an early **0.1.x** Merkle-DAG database
engine — expect breaking API changes until 1.0.

## Workflow (issue → PR)

For **non-trivial** changes (features, protocol/API work, multi-crate refactors,
process/CI beyond a one-liner):

1. **Open an issue first** using a [template](https://github.com/epochs-labs/epochs/issues/new/choose).
   Include problem, proposal, acceptance criteria, and non-goals.
2. Discuss / refine on the issue if needed.
3. Open a **PR that closes the issue** — put `Closes #N` in the PR body
   ([`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md)).

Small typos, trivial docs, and obvious one-line fixes can skip an issue.

## Before you open a PR

1. Read the [README](README.md) status table (what works vs roadmap).
2. Link the issue (`Closes #N`) unless the change is trivial.
3. Run locally:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

4. Keep diffs focused. Prefer small PRs over large refactors.
5. **Tests required** for new or changed behavior (`cargo test --workspace`
   must cover it — unit and/or integration). Do not land features without tests.
6. CI runs fmt/clippy/tests **and** coverage via **`cargo llvm-cov`**. The
   coverage job:
   - fails under [`.github/coverage-floor`](.github/coverage-floor)
   - fails if line coverage drops **>1pp** vs the latest successful `main` artifact
   - posts a sticky PR comment + Actions job summary
7. Do not commit secrets, local `target/`, or `benches/out/` scratch files.
   Published bench charts/CSV under `benches/charts/` and `benches/RESULTS.md`
   are fine when regenerating intentionally.

### Coverage locally

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
cargo llvm-cov --workspace --exclude epochs-bench --html --open
```

### Branch protection (maintainers)

Require these checks on `main`:

- `fmt · clippy · test`
- `coverage`

## Scope

| Area | Notes |
|------|--------|
| `epochs-core` | Storage engine (CAS, HAMT, commits) — keep **domain-free** |
| `epochql` | Query language + migrations |
| `epochs-cli` | Developer CLI |
| `epochs-server` | Epoch Protocol (EPX) TCP server |
| `epochs-client` | Async EPX client + pool |
| `epochs-bench` | Fair Docker benchmarks only |

Agent product / SaaS concerns belong **outside** this repo unless they are thin
examples. Server protocol and auth-shaped plumbing may live in `epochs-server`.


## License

By contributing, you agree your contributions are dual-licensed under
**MIT OR Apache-2.0**, the same as the rest of the project.
