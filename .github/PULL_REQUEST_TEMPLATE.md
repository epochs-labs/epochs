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

CI uploads coverage to Codecov and fails the **coverage** check if project
coverage drops materially or the patch is under-tested. Glance at the Codecov
comment / check on this PR before merge.

## Notes

<!-- Breaking changes, follow-ups, screenshots — optional. -->
