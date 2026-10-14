# Kudu for macOS

The native macOS app: Swift, SwiftUI and AppKit, living in the menu bar. All commands below
run from this `macos/` folder.

## Requirements
- macOS 14 or later, on Apple Silicon.
- Swift 6.2+ toolchain. The Command Line Tools are enough; Xcode is optional and can open
  `Package.swift` directly.

## Build & run
```bash
scripts/build-app.sh          # → build/Kudu.app
open build/Kudu.app
```
On first launch Kudu asks for **Microphone** and **Accessibility** access and downloads the
speech model.

### Stable signing for development
macOS remembers permissions per code signature. Ad-hoc builds get a new signature every time,
so the Accessibility grant stops matching after a rebuild. Create a local certificate once
and the build script uses it automatically:
```bash
scripts/setup-dev-signing.sh
```
Or sign with any certificate you already have:
```bash
KUDU_SIGN_IDENTITY="My Dev Cert" scripts/build-app.sh
```

## Test & benchmark
```bash
swift test                                       # cleanup pipeline unit tests
swift run -c release kudu-bench clip.wav ...     # per-clip latency through the real engine
open build/Kudu.app --args --overlay-demo        # preview the overlay pill in every state
```

## Layout
| Path | What |
|---|---|
| `Sources/KuduCore` | Pure logic: text-cleanup pipeline and 🌐-key guard, unit-tested |
| `Sources/KuduKit` | System integration: hotkey, audio, Parakeet engine, insertion, overlay, storage |
| `Sources/Kudu` | Menu-bar app, onboarding and settings |
| `Sources/KuduBench` | Command-line latency harness |
| `Tests/` | Unit tests (Swift Testing) |
| `scripts/` | Build and signing scripts |

How it all fits together: [ARCHITECTURE.md](ARCHITECTURE.md).
