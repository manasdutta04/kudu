<div align="center">

<img src="public/kudu-icon.png" alt="Kudu logo" width="128" height="128" />

# Kudu

**Private, fast voice dictation for macOS, Windows and Linux.**<br />
Hold a key, speak, release: clean text appears wherever your cursor is, in any app.<br />
Everything runs on your computer. No account, no cloud, no subscription.

[![Latest release](https://img.shields.io/github/v/release/manasdutta04/kudu?style=flat-square&color=0a84ff)](https://github.com/manasdutta04/kudu/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/manasdutta04/kudu/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/manasdutta04/kudu/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-22c55e?style=flat-square)](LICENSE)
[![Downloads](https://img.shields.io/github/downloads/manasdutta04/kudu/total?style=flat-square&color=8b5cf6)](https://github.com/manasdutta04/kudu/releases)
[![Stars](https://img.shields.io/github/stars/manasdutta04/kudu?style=flat-square&color=f59e0b)](https://github.com/manasdutta04/kudu/stargazers)
<br />
[![macOS](https://img.shields.io/badge/macOS-stable-000000?style=flat-square&logo=apple&logoColor=white)](#install)
[![Windows](https://img.shields.io/badge/Windows-stable-0078D6?style=flat-square&logo=windows&logoColor=white)](#install)
[![Linux](https://img.shields.io/badge/Linux-beta-FCC624?style=flat-square&logo=linux&logoColor=black)](#install)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-ff69b4?style=flat-square)](CONTRIBUTING.md)
[![100% on-device](https://img.shields.io/badge/privacy-100%25%20on--device-14b8a6?style=flat-square)](PRIVACY.md)

[**Download**](https://github.com/manasdutta04/kudu/releases/latest) ·
[Features](#features) ·
[Why Kudu](#why-kudu) ·
[Roadmap](docs/ROADMAP.md) ·
[Contributing](CONTRIBUTING.md) ·
[Report a bug](https://github.com/manasdutta04/kudu/issues/new/choose)

</div>

---

## Highlights

| | |
|---|---|
| ⚡ **Fast** | Text lands ~100–700 ms after you release the key. Kudu formats while you speak. |
| 🔒 **Private** | Your audio never leaves your computer. No telemetry, no analytics, no servers. |
| 🪶 **Light** | ~14 MB app plus a one-time model download (0.23–0.98 GB). Works offline after that. |
| 🧩 **Native** | A real native app per platform (Swift on the Mac, Rust on Windows and Linux), not a web wrapper. |
| 💸 **Free and open source** | MIT licensed. No account, no API keys, no monthly bill. |

## Why Kudu

Most dictation apps send your voice to someone else's servers and charge you every month for
it. Kudu does the whole job on your own machine, and it's free.

| | **Kudu** | Wispr Flow | Superwhisper |
|---|:---:|:---:|:---:|
| Speech recognition on your computer | ✅ Always | ❌ Cloud | ✅ |
| Formatting and rewriting on your computer | ✅ Always | ❌ Cloud | ⚠️ Depends on the model you pick |
| Works fully offline | ✅ | ❌ | ✅ |
| Open source | ✅ MIT | ❌ | ❌ |
| Price | **Free** | Free tier, $15/mo Pro | Free tier, $8.49/mo Pro |
| macOS · Windows | ✅ · ✅ | ✅ · ✅ | ✅ · ✅ |
| Linux | ✅ (beta) | ❌ | ❌ |

<sub>Comparison based on each product's public site as of October 2026. Spot something out of date? [Open an issue](https://github.com/manasdutta04/kudu/issues/new/choose).</sub>

## Install

| Platform | Status | Requirements |
|---|---|---|
| 🍎 **macOS** | ✅ Stable | Apple Silicon (M1 or newer), macOS 14+ |
| 🪟 **Windows** | ✅ Stable | Windows 10 or 11, x64 |
| 🐧 **Linux** | 🧪 Beta | x86_64, X11 or Wayland |

Grab a package from [Releases](https://github.com/manasdutta04/kudu/releases/latest), or use one command:

<details open>
<summary><b>macOS</b></summary>

```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install-macos.sh | bash
```
Installed from the `.dmg` by hand? Run this once so macOS opens the app:
```bash
chmod -R u+w /Applications/Kudu.app && xattr -cr /Applications/Kudu.app
```
</details>

<details>
<summary><b>Windows</b></summary>

In PowerShell:
```powershell
irm https://github.com/manasdutta04/kudu/releases/latest/download/install.ps1 | iex
```
Or run `Kudu-Windows-x64-Setup.exe`. If SmartScreen appears, click **More info → Run anyway**.
</details>

<details>
<summary><b>Linux (beta)</b></summary>

`.deb` and `.rpm` packages are on the release page, or:
```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install.sh | sh
```
It asks for `sudo` once so Kudu can read the keyboard and type. Log out and back in afterwards.
</details>

> The apps aren't signed with paid certificates yet, so the install commands also clear the
> "unidentified developer" warning.

## Features

- 🎙️ **Hold to talk, anywhere.** Works in every app. Double-tap for hands-free; Escape cancels.
- ✨ **Smart formatting while you speak.** Lists, numbers, times, paragraphs and email layout,
  without changing your words. Removes "um" and "uh" and applies corrections
  ("Tuesday, no, Wednesday" → "Wednesday").
- 🪄 **Command mode.** Select text and say "make this more formal", "translate to Hindi" or
  "summarize". With nothing selected, say what to write and Kudu drafts it.
- 🎨 **Styles per app.** Formal, casual or very casual for messages and email.
- 🧠 **Learns your words.** Fix a name once and Kudu remembers it. Add your own words and snippets.
- 📝 **Meeting notes (opt-in).** Transcribes Zoom, Teams, Meet and more on your computer, then
  writes the summary, decisions and action items. The audio is never saved.
- 📊 **Insights.** Words per minute, time saved and a streak heat map, all stored locally.

See the [roadmap](docs/ROADMAP.md) for what's next.

## How it works

All of the AI runs on your computer. After a one-time model download, Kudu needs no network.

| Job | macOS | Windows and Linux |
|---|---|---|
| Speech → text | Kudu Tiny / Kudu Base on the Apple Neural Engine | kudu-speech-tiny / kudu-speech-base on the CPU |
| Formatting and Command mode | Apple Intelligence (on-device) | [kudu-writer-tiny](https://huggingface.co/manasdutta04/kudu-writer-tiny) / [kudu-writer-base](https://huggingface.co/manasdutta04/kudu-writer-base) |
| Cleanup, styles, dictionary | Fast deterministic rules | The same rules, in Rust |

The Kudu writers are small models fine-tuned for one job: formatting dictation without
changing your words. Platform details live in [`macos/README.md`](macos/README.md) and
[`desktop/README.md`](desktop/README.md).

## Build from source

```bash
git clone https://github.com/manasdutta04/kudu.git && cd kudu

# macOS
cd macos && scripts/build-app.sh && open build/Kudu.app

# Windows and Linux
cd desktop && cargo run --release -p kudu-app
```

| Path | What |
|---|---|
| [`macos/`](macos) | The macOS app (Swift, SwiftUI, AppKit). See its [architecture](macos/ARCHITECTURE.md). |
| [`desktop/`](desktop) | The Windows and Linux app (Rust) |
| [`docs/`](docs) | Project-wide docs, such as the [roadmap](docs/ROADMAP.md) |

## Community

- 🐛 Found a bug or have an idea? [Open an issue](https://github.com/manasdutta04/kudu/issues/new/choose).
- 🤝 Want to help? Read [CONTRIBUTING.md](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).
- 🔐 Found a security problem? Please report it privately: see [SECURITY.md](SECURITY.md).
- 🛡️ What happens with your data: [PRIVACY.md](PRIVACY.md). Release notes: [CHANGELOG.md](CHANGELOG.md).

## Credits

Kudu stands on great open work:
[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) (CC-BY-4.0),
[Moonshine](https://huggingface.co/moonshine-ai/moonshine-base) (MIT),
[SmolLM2](https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct) and
[Qwen2.5](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct) (Apache-2.0, base models for the Kudu writers),
[FluidAudio](https://github.com/FluidInference/FluidAudio) and
[sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (Apache-2.0), and
[llama.cpp](https://github.com/ggml-org/llama.cpp) (MIT).

## License

[MIT](LICENSE) © 2026 Manas Dutta

<div align="center">
<br />
<b>Built by <a href="https://github.com/manasdutta04">Manas Dutta</a>.</b><br />
If Kudu saves you time, a ⭐ on the repo helps others find it.
</div>
