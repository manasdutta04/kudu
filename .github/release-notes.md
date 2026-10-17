Private, fast voice dictation. Hold a key, speak, release: clean text appears wherever your cursor is. Speech recognition runs on your own computer. No account, no cloud, no telemetry.

## Install

The apps aren't signed with paid Apple / Microsoft certificates yet, so your system will warn that they come from an unidentified developer. The commands below install Kudu and clear that warning. They're also in each download's install script, which you can read before running.

### macOS (Apple Silicon, macOS 14+)

```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install-macos.sh | bash
```

Or by hand: download **Kudu-macOS.dmg** (or the .zip), drag **Kudu** into **Applications**, then run this once in Terminal so macOS lets the unsigned app open:

```bash
chmod -R u+w /Applications/Kudu.app && xattr -cr /Applications/Kudu.app
```

On first launch, allow **Microphone** and **Accessibility** when Kudu asks. Hold **fn** to dictate.

### Windows 10 / 11 (64-bit) · beta

In PowerShell:

```powershell
irm https://github.com/manasdutta04/kudu/releases/latest/download/install.ps1 | iex
```

Or download the installer, **Kudu-Windows-x64-Setup.exe** (when SmartScreen appears, click **More info → Run anyway**), or the portable **Kudu-Windows-x64.zip** (in PowerShell in its folder run `Unblock-File .\kudu.exe`). Hold **Ctrl + Win** to dictate.

### Linux (x86_64, X11 and Wayland) · beta

```bash
curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install.sh | sh
```

The script installs Kudu to `~/.local/bin` and the app menu, and asks for `sudo` once to let Kudu read the keyboard (`input` group) and type through a virtual keyboard (`/dev/uinput`). Log out and back in after the first install. Hold **Ctrl + Super** to dictate.

Or install a package: `sudo apt install ./Kudu-Linux-x86_64.deb` (Debian, Ubuntu, Mint) or `sudo dnf install ./Kudu-Linux-x86_64.rpm` (Fedora, openSUSE), then add yourself to the input group once: `sudo usermod -aG input $USER` and log out and in. Any other distribution: extract **Kudu-Linux-x86_64.tar.gz** and run `./kudu/kudu`.

## First launch

Kudu downloads its speech model once (Mac ~450 MB; Windows/Linux: Accurate 482 MB or Compact 251 MB), then works offline. On Windows and Linux, smart formatting and Command mode are an optional extra download in Settings (491 MB or 1.1 GB).

## What's in this release

**New ways to install:** a Windows installer, and `.deb` and `.rpm` packages for Linux. Also included from 0.1.1: Windows and Linux no longer close when downloading the speech model; problems show an error and are written to `kudu.log` (Windows: `%LOCALAPPDATA%\Kudu\data\kudu.log`, Linux: `~/.local/share/kudu/kudu.log`).

New to Kudu? This is still the first public version:

- **macOS:** hold fn (or any shortcut you set) to dictate; formatting while you speak with Apple's on-device model (lists, numbers, units, math, @mentions, email layout, code and terminal commands); Command mode to rewrite or draft text; styles per app; a personal dictionary that learns from your corrections, with an Undo notice; names and terms spelled the way they appear on screen; snippets; history and Insights with a streak heat map. Apple Silicon only.
- **Windows and Linux (beta):** the same text cleanup, styles, dictionary, snippets, learning and shortcuts as the Mac app, with on-device speech (Parakeet or Moonshine) and an optional small language model (Qwen2.5). Please report anything that doesn't work on your setup.

Files are listed with SHA-256 checksums in `SHA256SUMS.txt`.
