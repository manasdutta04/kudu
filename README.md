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
xattr -cr /Applications/Kudu.app
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
- **History** saved only on your Mac, searchable by day, with the raw transcript.
- A small frosted-glass pill with a live waveform; sounds; open at login.

See the [roadmap](docs/ROADMAP.md) for what's next.

## The AI inside (all on your Mac)
| Job | What runs it |
|---|---|
| Speech → text | NVIDIA **Parakeet TDT 0.6B v2** on the Apple Neural Engine, through FluidAudio |
| Formatting and Command mode | Apple's on-device **Foundation Models** (the ~3B model behind Apple Intelligence, macOS 26+) |
| Cleanup, styles, dictionary, learning | Fast deterministic rules, written in Swift |
| Names and terms on screen | Accessibility + the system spell checker, to spell what you said the way it appears |

There are no servers, no accounts and no API keys. After the one-time model download, Kudu
works offline.

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
