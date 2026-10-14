# Kudu for Windows and Linux

Hold a shortcut, speak, release: the text appears wherever your cursor is. Speech recognition
runs on your PC; nothing is sent anywhere.

This is the Rust version of Kudu for **Windows 10/11** and **Linux** (X11 and Wayland). The Mac
app is in [`../macos`](../macos).

## What you download

| | Size |
|---|---|
| Kudu itself | one executable, no installer needed |
| Speech model, **Accurate** (Parakeet 0.6B, recommended) | 482 MB download, 631 MB on disk |
| Speech model, **Compact** (Moonshine Base) | 251 MB download, 272 MB on disk |
| Optional smart formatting, **Small** (Qwen2.5 0.5B) | 491 MB |
| Optional smart formatting, **Better** (Qwen2.5 1.5B) | 1.1 GB |

Kudu downloads the speech model you choose the first time it starts. Smart formatting (lists,
numbers, email layout, Command mode) is optional: without it, Kudu still cleans up fillers,
corrections, spoken punctuation, times, file names, names from your dictionary and more, with
fast rules.

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
                 the Mac app's KuduCore (147 tests).
  kudu-engine/   Microphone capture (cpal), speech recognition (sherpa-onnx, ONNX Runtime on
                 the CPU), the optional small language model (llama.cpp), model downloads.
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
cargo run --release -p kudu-engine --example format -- ./models/qwen2.5-1.5b-instruct-q4_k_m.gguf
```
