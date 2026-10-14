# Contributing to Kudu

Thanks for helping make dictation on the Mac private and fast.

## Ground rules
- **Nothing leaves the Mac.** No network calls apart from the one-time model download, no
  analytics, no crash reporters. PRs that add any of these will not be merged.
- **Latency is a feature.** Anything on the path from key release to inserted text needs
  numbers: run `kudu-bench` before and after.
- **Keep it light.** New dependencies need a strong reason and a permissive license
  (MIT, Apache-2.0, BSD).

## Getting started
```bash
git clone https://github.com/manasdutta04/kudu.git
cd kudu
swift test
scripts/build-app.sh && open build/Kudu.app
```
Only the Command Line Tools are required. Xcode can open `Package.swift` directly if you prefer.

While developing, sign with a stable certificate so macOS keeps the Accessibility grant
across rebuilds:
```bash
KUDU_SIGN_IDENTITY="My Dev Cert" scripts/build-app.sh
```

## Where things live
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). In short:
- Text cleanup rules go in `Sources/KuduCore/Stages` as a `TextStage`. They are pure and must have tests.
- Anything touching macOS APIs goes in `Sources/KuduKit`.
- UI goes in `Sources/Kudu`.

## Pull requests
- One topic per PR, with a short description of the user-visible change.
- `swift test` must pass. Add tests for new cleanup behavior, including cases that must **not** change.
- Use [Conventional Commits](https://www.conventionalcommits.org) for messages (`feat(core): …`, `fix(insert): …`).
- SwiftUI note: avoid `@State`, `@Observable` and `#Preview`. They are macros, and the
  Command Line Tools ship without the SwiftUI macro plugin. Use `ObservableObject` instead.

## Reporting bugs
Open an issue with your macOS version, Mac model, the app you were dictating into, and what
happened. Text that went to the wrong place or didn't appear is especially useful to report.
