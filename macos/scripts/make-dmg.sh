#!/usr/bin/env bash
# Packs Kudu.app into a disk image whose Finder window shows the app, an arrow and the
# Applications folder on a designed background (see macos/packaging/dmg-settings.py).
#
#   macos/scripts/make-dmg.sh build/Kudu.app ../Kudu-macOS.dmg     (from macos/)
#
# dmgbuild writes the window layout itself, so this doesn't need Finder or AppleScript and
# works the same on a CI runner.
set -euo pipefail

APP="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
OUT="$2"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
DMGBUILD_VERSION="1.6.7"

[ -d "$APP" ] || { echo "No app at $APP" >&2; exit 1; }

VENV="$(mktemp -d)/venv"
python3 -m venv "$VENV"
"$VENV/bin/pip" install --quiet --disable-pip-version-check "dmgbuild==$DMGBUILD_VERSION"

rm -f "$OUT"
"$VENV/bin/dmgbuild" \
  -s "$HERE/packaging/dmg-settings.py" \
  -D app="$APP" \
  -D packaging="$HERE/packaging" \
  -D volume_icon="$HERE/Resources/AppIcon.icns" \
  Kudu "$OUT"

rm -rf "$(dirname "$VENV")"
hdiutil verify "$OUT" >/dev/null
echo "Wrote $OUT"
