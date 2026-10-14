# Kudu

**Private, fast voice dictation for macOS.** Hold **fn**, speak, release: clean text appears
wherever your cursor is, in any app. Everything runs on your Mac. No account, no cloud, no
telemetry.

| | |
|---|---|
| ⚡ **Fast** | Text lands ~50–80 ms after you release the key (measured on M4 with 4–13 s clips) |
| 🔒 **Private** | Speech recognition runs on the Apple Neural Engine; audio never leaves memory |
| 🪶 **Light** | ~11 MB app; the speech model (~450 MB) downloads once, then works offline |
| 🍎 **Native** | Swift, SwiftUI and AppKit. Lives in the menu bar |

## Features
- **Hold to talk** on fn, Right ⌘ or Right ⌥. Double-tap for hands-free; Escape cancels.
- **Works in every app.** Uses Accessibility insertion where it works and a clipboard paste
  elsewhere. Your clipboard is restored afterwards, and the paste key follows your keyboard layout.
- **Cleans up as you speak:**
  - Removes "um" and "uh".
  - Applies corrections: "Tuesday, no, Wednesday" → "Wednesday".
  - Understands "new line" and "new paragraph".
  - Writes times properly ("10:30").
  - Fits capitalization and spacing to the text around the cursor, and is code-aware in editors and terminals.
- **Personal dictionary** for names and jargon the model mishears.
- **Snippets:** say "my email" and get your address. Supports `{date}`, `{time}` and `{clipboard}`.
- **History** of recent dictations, including the raw transcript, kept in memory only.
- Sounds, open at login, a warning when your mic is silent.

See the [roadmap](docs/ROADMAP.md) for what's next.

## Requirements
- macOS 14 or later, on Apple Silicon.
- Swift 6.2+ toolchain. The Command Line Tools are enough; Xcode is optional.

## Build & run
```bash
scripts/build-app.sh          # → build/Kudu.app
open build/Kudu.app
```
On first launch Kudu asks for **Microphone** and **Accessibility** access and downloads the
speech model.

Ad-hoc signed builds lose their Accessibility grant on every rebuild. While developing, sign
with a stable certificate:
```bash
KUDU_SIGN_IDENTITY="My Dev Cert" scripts/build-app.sh
```

## Test & benchmark
```bash
swift test                                       # cleanup pipeline unit tests
swift run -c release kudu-bench clip.wav ...     # per-clip latency through the real engine
```

## Project
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
- [Privacy](PRIVACY.md)
- [Contributing](CONTRIBUTING.md)

## Credits
- [FluidAudio](https://github.com/FluidInference/FluidAudio): Apache-2.0.
- [NVIDIA Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2): CC-BY-4.0, by NVIDIA. Kudu uses FluidInference's Core ML conversion.

## License
MIT. See [LICENSE](LICENSE).
