# Kudu for macOS

The native macOS app: Swift, SwiftUI and AppKit, living in the menu bar. All commands below
run from this `macos/` folder.

## Requirements
- macOS 14 or later, on Apple Silicon (M1 or newer). Intel Macs are not supported: Kudu relies
  on the Neural Engine for speech and on Apple Intelligence for formatting.
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
swift test                                       # unit tests
swift run -c release kudu-bench clip.wav ...     # per-clip latency through the real engine
open build/Kudu.app --args --overlay-demo        # preview the overlay pill in every state
swift run -c release kudu-bench --format --text "so the plan is first we ship"   # try smart formatting
```

## App icon
The icon is generated from the master logo `public/kudu-icon.png`. Re-run after changing the
artwork (from the repository root):
```bash
swift macos/scripts/make-app-icon.swift
```

## Layout
| Path | What |
|---|---|
| `Sources/KuduCore` | Pure logic: cleanup pipeline, formatting guard, usage stats, 🌐-key guard, unit-tested |
| `Sources/KuduKit` | System integration: hotkey, audio, Parakeet engine, smart formatting, insertion, overlay, history, branding |
| `Sources/Kudu` | The app: main window (Home, Dictionary, Snippets, Style, Settings), menu bar, onboarding |
| `Sources/KuduBench` | Command-line latency and formatting harness |
| `Resources/` | App icon, generated from `public/kudu-icon.png` |
| `Tests/` | Unit tests (Swift Testing) |
| `scripts/` | Build and signing scripts |

How it all fits together: [ARCHITECTURE.md](ARCHITECTURE.md).
