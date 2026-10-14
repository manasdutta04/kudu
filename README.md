# Kudu

**Private, fast voice dictation.** Hold a key, speak, release: clean text appears wherever
your cursor is, in any app. Speech recognition runs on your own computer. No account, no
cloud, no telemetry.

| | |
|---|---|
| ⚡ **Fast** | Text lands ~100–700 ms after you release the key, even for long, formatted dictations: Kudu formats while you speak (Apple M4) |
| 🔒 **Private** | Audio never leaves your computer |
| 🪶 **Light** | ~14 MB app; the speech model (~450 MB) downloads once, then works offline |
| 🧩 **Native** | A native app per platform, not a web wrapper |

## Platforms

| Platform | Status | Folder |
|---|---|---|
| macOS 14+ (Apple Silicon) | ✅ Available (early) | [`macos/`](macos) (Swift, SwiftUI, AppKit) |
| Windows | 🗓 Planned | `windows/` (Rust) |
| Linux | 🗓 Planned | `linux/` (Rust) |

## Features
- **A real Mac app:** a Dock icon, a Home dashboard with your words, speed, streak and full
  history, plus an animated menu-bar icon that moves with your voice.
- **Smart formatting on device, while you speak:** numbered lists, numbers written properly
  ("three thirty" → 3:30), letter layout and paragraphs, using Apple's on-device model, without
  changing your words.
- **Command mode (fn + ⌃):** select text and say "make this more formal", "translate to
  Hindi" or "summarize"; with nothing selected, say what to write and Kudu drafts it.
- **Styles per app:** Formal, Casual or Very casual for personal messages, work messages and
  email, plus an editing strength from Verbatim to Polished.
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
- **History** saved only on your Mac, searchable by day, with the raw transcript.
- A small frosted-glass pill with a live waveform; sounds; open at login.

See the [roadmap](docs/ROADMAP.md) for what's next.

## Quick start (macOS)
```bash
cd macos
scripts/build-app.sh && open build/Kudu.app
```
See [macos/README.md](macos/README.md) for requirements, signing, tests and benchmarks.

## Repository layout
| Path | What |
|---|---|
| [`macos/`](macos) | The macOS app: Swift package, scripts, tests, [architecture](macos/ARCHITECTURE.md) |
| [`docs/`](docs) | Project-wide docs, such as the [roadmap](docs/ROADMAP.md) |
| [`PRIVACY.md`](PRIVACY.md) | What Kudu does and doesn't do with your data |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | How to contribute |
| [`CHANGELOG.md`](CHANGELOG.md) | Release notes |

## Credits
- [FluidAudio](https://github.com/FluidInference/FluidAudio): Apache-2.0.
- [NVIDIA Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2): CC-BY-4.0, by NVIDIA. Kudu uses FluidInference's Core ML conversion.

## License
MIT. See [LICENSE](LICENSE).
