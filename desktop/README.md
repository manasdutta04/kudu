# Kudu for Windows and Linux

Hold a shortcut, speak, release: the text appears wherever your cursor is. Speech recognition
runs on your PC; nothing is sent anywhere.

This is the Rust version of Kudu for **Windows 10/11** and **Linux** (X11 and Wayland). The Mac
app is in [`../macos`](../macos).

## What you download

Kudu itself is one small executable (or the installer / `.deb` / `.rpm`). The first time it
starts you pick a **model pack**: a speech model plus a writer model, downloaded together and
checked against their sha256 checksums.

| Pack | Speech | Writer | Download | On disk |
|---|---|---|---|---|
| kudu-tiny | kudu-speech-tiny (Moonshine Base) | kudu-writer-tiny | 0.48 GB | 0.50 GB |
| **kudu-basic** (default) | kudu-speech-base (Parakeet TDT 0.6B v2) | kudu-writer-tiny | 0.71 GB | 0.86 GB |
| kudu-large | kudu-speech-base (Parakeet TDT 0.6B v2) | kudu-writer-base | 0.84 GB | 0.98 GB |

The writer is Kudu's own small model, fine-tuned to format dictation (lists, numbers, email
layout, code) and to run Command mode, without changing your words. Speech files come from the
sherpa-onnx releases; the writers from Hugging Face
([kudu-writer-tiny](https://huggingface.co/manasdutta04/kudu-writer-tiny),
[kudu-writer-base](https://huggingface.co/manasdutta04/kudu-writer-base)). The catalogue is
`crates/kudu-engine/src/packs.json`. You can switch packs, and remove the ones you no longer
use, in Settings → Models.

## Shortcuts

| | Default | |
|---|---|---|
| Dictate | Hold **Ctrl + Win** (Linux: Ctrl + Super) | Double-tap for hands-free |
| Command mode | Hold **Ctrl + Win + Alt** | Select text, say "make this more formal" |
| Paste last transcript | **Ctrl + Alt + V** | |
| Cancel | **Esc** while dictating | |

Change them in Settings → Shortcuts: click Change and press the keys you want.

## Build

You need [Rust](https://rustup.rs) (stable, 1.95 or newer).

### Windows

Install the Visual Studio Build Tools ("Desktop development with C++") and
[CMake](https://cmake.org/download/) (for the optional smart formatting).

```powershell
cd desktop
cargo build --release
.\target\release\kudu.exe
```

To build without smart formatting (no CMake needed): `cargo build --release --no-default-features`.

### Linux

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config cmake libasound2-dev libssl-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libdbus-1-dev
# Fedora
sudo dnf install gcc-c++ pkgconf cmake alsa-lib-devel openssl-devel libxkbcommon-devel \
  wayland-devel libX11-devel dbus-devel

cd desktop
cargo build --release
./target/release/kudu
```

Kudu reads the keyboard through `/dev/input` and types through `/dev/uinput`, which works on
X11 and every Wayland desktop. Allow it once (then log out and back in):

```bash
sudo usermod -aG input $USER
sudo cp packaging/linux/70-kudu-uinput.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

The settings window shows these steps too if anything is missing. The tray icon needs a desktop
with StatusNotifierItem support (KDE; GNOME with the AppIndicator extension). The overlay pill
uses layer-shell on KDE, Hyprland and Sway, and XWayland on GNOME.

## How it's built

```
crates/
  kudu-core/     The text pipeline, shortcuts and learning: pure Rust, the same behaviour as
                 the Mac app's KuduCore (153 tests).
  kudu-engine/   Microphone capture (cpal), speech recognition (sherpa-onnx, ONNX Runtime on
                 the CPU), the writer model (llama.cpp), the model pack catalogue and
                 checksum-verified downloads.
  kudu-app/      The app: a light background process (shortcuts, microphone, overlay pill,
                 tray) and the settings window (egui), started only when you open it.
    src/platform/windows/   Keyboard hook, clipboard paste, UI Automation, layered overlay
    src/platform/linux/     evdev, uinput, AT-SPI, layer-shell / X11 overlay, StatusNotifierItem
```

- **Fast on any CPU.** While you speak, Kudu cuts the recording at pauses and transcribes the
  finished phrases in the background, so when you let go only the last few seconds are left.
  The pieces are joined by word timings, re-hearing the words at each cut with the words after
  them.
- **Private.** No account, no telemetry. The network is used only to download models.

Try the engine on its own:

```bash
cargo run --release -p kudu-engine --no-default-features --example transcribe -- ./models parakeet recording.wav
cargo run --release -p kudu-engine --example format -- ./models/kudu-writer-tiny-v1-iq4_xs.gguf
# What the writer would receive for a dictation (rules, routing, exact prompt); no model needed:
cargo run -p kudu-engine --no-default-features --example writer_prompt -- --app Slack "so um three things first …"
```
