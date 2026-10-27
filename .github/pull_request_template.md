## What changes for users

<!-- One or two sentences. Link the issue: Fixes #123 -->

## Type of change
- [ ] Bug fix
- [ ] New feature
- [ ] Cleanup or formatting rule
- [ ] Docs or project setup

## Platforms affected
- [ ] macOS
- [ ] Windows
- [ ] Linux

## How it was tested
- [ ] `swift test` (in `macos/`)
- [ ] `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` (in `desktop/`)
- [ ] Dictated into: <!-- apps -->
- [ ] Latency before/after (`kudu-bench`), if this touches the dictation path

## Checklist
- [ ] Nothing new leaves the computer (no network calls, analytics or crash reporting)
- [ ] Tests added for new cleanup behaviour, including cases that must **not** change
- [ ] `CHANGELOG.md` updated, if users will notice
