## Summary

<!-- What does this PR change, and why? 1–3 bullets. -->

-

## Test plan

<!-- How should reviewers / CI verify this? Check what applies. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] New/changed behavior has unit or integration tests
- [ ] (If protocol/server) EPX or CLI smoke still passes

## Coverage

CI runs `cargo llvm-cov` (no third-party coverage SaaS). The **coverage** job
fails if line coverage drops below `.github/coverage-floor` or more than 1pp vs
the latest successful `main` baseline. Check the PR coverage comment / job summary.

## Notes

<!-- Breaking changes, follow-ups, screenshots — optional. -->

<!-- coverage CI smoke -->
