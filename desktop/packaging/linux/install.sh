#!/bin/sh
# Installs the latest Kudu for Linux for the current user.
#
#   curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install.sh | sh
#
# Puts `kudu` in ~/.local/bin, adds it to the app menu, and (with sudo, once) gives it the two
# permissions it needs on X11 and Wayland: reading the keyboard (/dev/input, the "input" group)
# and typing through a virtual keyboard (/dev/uinput). Run it again to update.
set -eu

REPO="manasdutta04/kudu"
ARCH=$(uname -m)
case "$ARCH" in
  x86_64 | amd64) ARCH=x86_64 ;;
  *) echo "Kudu for Linux is built for x86_64 for now (this PC is $ARCH)." >&2; exit 1 ;;
esac
URL="https://github.com/$REPO/releases/latest/download/Kudu-Linux-$ARCH.tar.gz"

BIN="$HOME/.local/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading Kudu…"
curl -fL --progress-bar "$URL" -o "$tmp/kudu.tar.gz"
tar -xzf "$tmp/kudu.tar.gz" -C "$tmp"
src="$tmp/kudu"

pkill -x kudu 2>/dev/null && sleep 1 || true
mkdir -p "$BIN" "$APPS" "$ICONS"
install -m 755 "$src/kudu" "$BIN/kudu"
install -m 644 "$src/kudu.png" "$ICONS/kudu.png"
sed "s|^Exec=kudu|Exec=$BIN/kudu|" "$src/kudu.desktop" > "$APPS/kudu.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" >/dev/null 2>&1 || true

# Missing system libraries (sound, TLS for downloads, OpenMP for smart formatting).
missing=$(ldd "$BIN/kudu" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')
if [ -n "$missing" ]; then
  echo
  echo "Kudu needs these libraries, which aren't installed: $missing"
  echo "  Debian/Ubuntu: sudo apt install libasound2 libssl3 libgomp1"
  echo "  Fedora:        sudo dnf install alsa-lib openssl-libs libgomp"
  echo "  Arch:          sudo pacman -S alsa-lib openssl gcc-libs"
fi

# Permissions: keyboard access and the virtual keyboard.
need_relogin=0
if ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
  echo
  echo "Kudu needs to read the keyboard to notice its shortcut. Adding you to the 'input' group (sudo)…"
  sudo usermod -aG input "$USER" && need_relogin=1
fi
if [ ! -f /etc/udev/rules.d/70-kudu-uinput.rules ]; then
  echo "Allowing Kudu to type through a virtual keyboard (sudo)…"
  sudo install -m 644 "$src/70-kudu-uinput.rules" /etc/udev/rules.d/70-kudu-uinput.rules
  sudo modprobe uinput 2>/dev/null || true
  sudo udevadm control --reload-rules && sudo udevadm trigger
fi

echo
echo "Kudu is installed: $BIN/kudu (and in your app menu)."
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Note: $BIN isn't on your PATH; start Kudu from the app menu." ;; esac
if [ "$need_relogin" = 1 ]; then
  echo "Log out and back in once, so the new 'input' group takes effect. Then start Kudu."
else
  "$BIN/kudu" >/dev/null 2>&1 &
  echo "Kudu is starting. Hold Ctrl + Super to dictate."
fi
