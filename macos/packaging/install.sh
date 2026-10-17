#!/bin/bash
# Installs the latest Kudu for Mac into /Applications.
#
#   curl -fsSL https://github.com/manasdutta04/kudu/releases/latest/download/install-macos.sh | bash
#
# Kudu isn't signed with a paid Apple Developer certificate, so macOS quarantines the download
# and refuses to open it ("Kudu is damaged" / "can't be verified"). This script removes the
# quarantine flag (xattr -cr), which is what makes an unsigned app open normally.
set -euo pipefail

REPO="manasdutta04/kudu"
URL="https://github.com/$REPO/releases/latest/download/Kudu-macOS.zip"
APP="/Applications/Kudu.app"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is for macOS." >&2
  exit 1
fi
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "Kudu for Mac needs Apple Silicon (M1 or newer). Intel Macs aren't supported." >&2
  exit 1
fi
major=$(sw_vers -productVersion | cut -d. -f1)
if (( major < 14 )); then
  echo "Kudu needs macOS 14 Sonoma or newer (this Mac has $(sw_vers -productVersion))." >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading Kudu…"
curl -fL --progress-bar "$URL" -o "$tmp/Kudu.zip"
ditto -x -k "$tmp/Kudu.zip" "$tmp"

if pgrep -x Kudu >/dev/null; then
  echo "Quitting the running Kudu…"
  osascript -e 'tell application id "app.kudu.Kudu" to quit' >/dev/null 2>&1 || true
  sleep 2
fi
rm -rf "$APP"
mv "$tmp/Kudu.app" "$APP"
# Unsigned app: clear the quarantine flag so macOS lets it open (read-only files inside
# would keep it, so make them writable first).
chmod -R u+w "$APP"
xattr -cr "$APP"

echo "Kudu is installed in /Applications."
echo "On first launch, allow Microphone and Accessibility when Kudu asks (System Settings → Privacy & Security)."
open "$APP"
