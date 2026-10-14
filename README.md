# Kudu

Private, fast voice dictation for macOS. Hold **fn**, speak, release: clean text appears
wherever your cursor is, in any app. Everything runs on your Mac. No account, no cloud and
no telemetry.

- **On-device speech recognition:** NVIDIA Parakeet TDT 0.6B v2 on the Apple Neural Engine (via FluidAudio).
- **Fast:** text arrives about 50–80 ms after you release the key (M4, 4–13 s clips).
- **Light:** the app is about 11 MB. The speech model (about 450 MB) downloads once on first launch.
- **Native:** Swift, SwiftUI and AppKit. Lives in the menu bar.


## Requirements
- macOS 14 or later, on Apple Silicon.
- Swift 6.2+ toolchain. The Command Line Tools are enough; Xcode is not required.

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
swift test                                       # text-pipeline unit tests
swift run -c release kudu-bench clip.wav ...     # per-clip latency through the real engine
```

## Layout
| Path | What |
|---|---|
| `Sources/KuduCore` | Pure text-cleanup pipeline, unit-tested |
| `Sources/KuduKit` | Hotkey (CGEventTap), audio capture, Parakeet engine, text insertion, overlay, permissions |
| `Sources/Kudu` | Menu-bar app and onboarding |
| `Sources/KuduBench` | Command-line latency harness |

## Credits
- [FluidAudio](https://github.com/FluidInference/FluidAudio): Apache-2.0.
- [NVIDIA Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2): CC-BY-4.0, by NVIDIA. Kudu uses FluidInference's Core ML conversion.

## License
MIT. See [LICENSE](LICENSE).
