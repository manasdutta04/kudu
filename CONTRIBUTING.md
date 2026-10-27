# Contributing to Kudu

Thanks for helping make dictation private and fast on every computer. Whether it's a bug
report, a fix, a new cleanup rule or a docs tweak, every contribution helps.

By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ground rules

- **Nothing leaves the computer.** No network calls apart from the one-time model download and
  the update check, no analytics, no crash reporters. PRs that add any of these won't be merged.
- **Latency is a feature.** Anything on the path from key release to inserted text needs
  numbers: measure before and after (`kudu-bench` on the Mac).
- **Keep it light.** New dependencies need a strong reason and a permissive license
  (MIT, Apache-2.0, BSD).

## Ways to contribute

- 🐛 **Report a bug** with the [bug report form](https://github.com/manasdutta04/kudu/issues/new?template=bug_report.yml).
- ✍️ **Report wrong text** (a misheard word, bad formatting) with the
  [transcription form](https://github.com/manasdutta04/kudu/issues/new?template=transcription_issue.yml).
- 💡 **Suggest a feature** with the [feature request form](https://github.com/manasdutta04/kudu/issues/new?template=feature_request.yml).
- 🧑‍💻 **Send a pull request.** Issues labelled `good first issue` are a great place to start.
- 🔐 **Security problems** go through [SECURITY.md](SECURITY.md), never a public issue.

## Getting started

```bash
git clone https://github.com/manasdutta04/kudu.git
cd kudu
```

### macOS app (`macos/`, Swift)

```bash
cd macos
swift test
scripts/build-app.sh && open build/Kudu.app
```
Only the Command Line Tools are required; Xcode can open `macos/Package.swift` if you prefer.
Run `scripts/setup-dev-signing.sh` once so macOS keeps the Accessibility grant across rebuilds
(details in [macos/README.md](macos/README.md)).

### Windows and Linux app (`desktop/`, Rust)

```bash
cd desktop
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p kudu-app
```
On Linux, install the build dependencies first (Debian/Ubuntu):
```bash
sudo apt-get install pkg-config cmake libasound2-dev libssl-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libdbus-1-dev libudev-dev
```
See [desktop/README.md](desktop/README.md) for details.

## Where things live

| Path | What |
|---|---|
| `macos/Sources/KuduCore/Stages` | Text cleanup rules (`TextStage`). Pure, and must have tests. |
| `macos/Sources/KuduKit` | Anything touching macOS APIs |
| `macos/Sources/Kudu` | Mac UI |
| `desktop/crates/kudu-core` | Cleanup rules and shared logic in Rust (mirrors `KuduCore`) |
| `desktop/crates/kudu-app` | The Windows and Linux app, with platform code in `src/platform/` |
| `docs/` | Project-wide docs |

For the Mac app's design, see [macos/ARCHITECTURE.md](macos/ARCHITECTURE.md). A cleanup
change usually belongs on both platforms, so keep the Swift and Rust rules in step.

## Pull requests

`main` is protected: nobody pushes to it directly, every change lands through a pull request.

1. Fork the repo and create a branch from `main` (`fix/paste-in-terminal`, `feat/new-snippet`).
2. Keep each PR to one topic, with a short description of the user-visible change.
3. Make sure CI passes: `swift test` for `macos/`, `cargo test` and `cargo clippy` for `desktop/`.
4. Add tests for new cleanup behaviour, including cases that must **not** change.
5. Use [Conventional Commits](https://www.conventionalcommits.org) for messages
   (`feat(core): …`, `fix(insert): …`, `docs: …`).
6. Fill in the pull request template and link the issue it fixes (`Fixes #123`).

A maintainer reviews every PR. Small, focused PRs get merged fastest.

### SwiftUI note

Avoid `@State`, `@Observable` and `#Preview`. They're macros, and the Command Line Tools ship
without the SwiftUI macro plugin. Use `ObservableObject` instead.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
