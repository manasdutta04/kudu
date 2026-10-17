# Kudu

**Private, fast voice dictation.** Hold a key, speak, release: clean text appears wherever
your cursor is, in any app. Speech recognition runs on your own computer. No account, no
cloud, no telemetry.

| | |
|---|---|
| ⚡ **Fast** | Text lands ~100–700 ms after you release the key, even for long, formatted dictations: Kudu formats while you speak (Apple M4) |
| 🔒 **Private** | Audio never leaves your computer |
| 🪶 **Light** | ~14 MB app. Mac: Kudu Tiny (~230 MB) or Kudu Base (~450 MB) for speech, formatting by Apple Intelligence. Windows and Linux: one model pack (0.48–0.84 GB). Downloaded once, then offline |
| 🧩 **Native** | A native app per platform, not a web wrapper |

## Install

Download from [Releases](https://github.com/manasdutta04/kudu/releases/latest), or use one command.
The apps aren't signed with paid certificates yet; these commands install Kudu and clear the
"unidentified developer" warning.

**macOS** (Apple Silicon, macOS 14+)
```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install-macos.sh | bash
```
Installed by hand (dragged from the .dmg)? Run this once so macOS opens the unsigned app:
```bash
chmod -R u+w /Applications/Kudu.app && xattr -cr /Applications/Kudu.app
```

**Windows 10/11** (beta), in PowerShell
```powershell
irm https://github.com/manasdutta04/kudu/releases/latest/download/install.ps1 | iex
```
Or run the installer (`Kudu-Windows-x64-Setup.exe`). When SmartScreen appears, click **More info → Run
anyway**; for the portable zip, run `Unblock-File .\kudu.exe` in its folder.

**Linux** (beta, x86_64, X11 and Wayland): `.deb` and `.rpm` packages are on the release page, or
```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install.sh | sh
```
It asks for `sudo` once to let Kudu read the keyboard and type; log out and back in after.

## Platforms

| Platform | Status | Folder |
|---|---|---|
| macOS 14+ on Apple Silicon (M1 or newer; not Intel) | ✅ Available (early) | [`macos/`](macos) (Swift, SwiftUI, AppKit) |
| Windows 10/11 | 🧪 Beta: builds in CI, being tested | [`desktop/`](desktop) (Rust) |
| Linux (X11 and Wayland) | 🧪 Beta: builds in CI, being tested | [`desktop/`](desktop) (Rust) |

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
- **Hold to talk** on fn, or any shortcut you set in Settings (Right ⌥, ⌃⌥, ⌥Space…). Double-tap
  for hands-free; Escape cancels. Command mode and paste-last shortcuts are customizable too.
- **Works in every app.** Uses Accessibility insertion where it works and a clipboard paste
  elsewhere. Your clipboard is restored afterwards, and the paste key follows your keyboard layout.
- **Cleans up as you speak:**
  - Removes "um" and "uh".
  - Applies corrections: "Tuesday, no, Wednesday" → "Wednesday".
  - Understands "new line" and "new paragraph".
  - Writes times properly ("10:30").
  - Fits capitalization and spacing to the text around the cursor, and is code-aware in editors and terminals.
- **Learns from your corrections:** fix a name Kudu misspelled and it remembers it for next
  time (✦ in the Dictionary). Or add words yourself.
- **Insights:** words per minute, words cleaned up, time saved, a streak heat map and where
  you dictate.
- **Snippets:** say "my email" and get your address. Supports `{date}`, `{time}` and `{clipboard}`.
- **Meeting notes (off until you turn them on):**
  - Kudu transcribes your calls in Zoom, Teams, Meet or any other app, on your computer, and
    writes up the title, summary, decisions and action items when the call ends.
  - It hears you through the microphone and the other people through what your computer plays,
    labelling them "You" and "Others"; it never saves the audio.
  - Start and stop with ⌃⌥N (Ctrl + Alt + N on Windows and Linux), the menu bar or tray, or let
    Kudu ask or start by itself when a meeting app starts using the microphone.
  - Notes are written by Apple Intelligence on a Mac and by the Kudu writer on Windows and
    Linux. Without them, Kudu picks the action items and decisions from the transcript itself.
  - Meetings are kept on the Meetings page until you delete them; copy or export them as
    Markdown.
- **History** saved only on your Mac, searchable by day, with the raw transcript.
- A small frosted-glass pill with a live waveform; sounds; open at login.

See the [roadmap](docs/ROADMAP.md) for what's next.

## The AI inside (all on your computer)
| Job | Mac | Windows and Linux |
|---|---|---|
| Speech → text | **Kudu Tiny** (~230 MB) or **Kudu Base** (~450 MB) on the Apple Neural Engine, through FluidAudio | **kudu-speech-base** or **kudu-speech-tiny** (Moonshine Base) on the CPU, through sherpa-onnx |
| Formatting and Command mode | Apple Intelligence (Apple's on-device Foundation Models, macOS 26+); nothing extra to download | **kudu-writer-tiny** or **kudu-writer-base**, Kudu's own writer models, through llama.cpp |
| Cleanup, styles, dictionary, learning | Fast deterministic rules | The same rules, in Rust |

On Windows and Linux you pick one **model pack** at setup; speech and writer download together:

| Pack | Speech | Writer | Download | On disk |
|---|---|---|---|---|
| Kudu Tiny | kudu-speech-tiny | kudu-writer-tiny | 0.48 GB | 0.50 GB |
| **Kudu Base** (default) | kudu-speech-base | kudu-writer-tiny | 0.71 GB | 0.86 GB |
| Kudu Large | kudu-speech-base | kudu-writer-base | 0.84 GB | 0.98 GB |

The writers are small models fine-tuned for one job: formatting dictation without changing your
words. On Kudu's dictation test set they keep about 98% of their replies through the app's
safety check (a general-purpose model of the same size: 80%) and format a 50-word dictation in
about a second on a laptop CPU. They're published on Hugging Face:
[kudu-writer-tiny](https://huggingface.co/manasdutta04/kudu-writer-tiny) and
[kudu-writer-base](https://huggingface.co/manasdutta04/kudu-writer-base).

There are no servers, no accounts and no API keys. After the one-time download, Kudu works
offline.

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
- [NVIDIA Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) (kudu-speech-base; Kudu Base on the Mac): CC-BY-4.0, by NVIDIA.
- [NVIDIA Parakeet TDT-CTC 110M](https://huggingface.co/nvidia/parakeet-tdt_ctc-110m) (Kudu Tiny on the Mac): CC-BY-4.0, by NVIDIA, through FluidInference's Core ML conversion. On the Mac, FluidInference's Core ML conversion; on Windows and Linux, the sherpa-onnx int8 export.
- [Moonshine Base](https://huggingface.co/moonshine-ai/moonshine-base) (kudu-speech-tiny): MIT, by Useful Sensors / Moonshine AI.
- [kudu-writer-tiny](https://huggingface.co/manasdutta04/kudu-writer-tiny), fine-tuned from [SmolLM2-360M-Instruct](https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct), and [kudu-writer-base](https://huggingface.co/manasdutta04/kudu-writer-base), fine-tuned from [Qwen2.5-0.5B-Instruct](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct): Apache-2.0.
- [FluidAudio](https://github.com/FluidInference/FluidAudio), [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx): Apache-2.0. [llama.cpp](https://github.com/ggml-org/llama.cpp): MIT.

## License
MIT. See [LICENSE](LICENSE).
