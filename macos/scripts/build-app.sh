#!/usr/bin/env bash
# Builds Kudu.app from the Swift package. No Xcode project needed.
#
#   scripts/build-app.sh            # release build → build/Kudu.app
#   CONFIG=debug scripts/build-app.sh
#
# Signing: macOS ties Accessibility/Microphone grants to the code signature. Ad-hoc
# signatures change on every build, so permissions stop matching. Run
# scripts/setup-dev-signing.sh once and this script signs with "Kudu Development"
# automatically. Or pick any identity:
#   KUDU_SIGN_IDENTITY="Your Code Signing Cert" scripts/build-app.sh
set -euo pipefail

cd "$(dirname "$0")/.."
CONFIG="${CONFIG:-release}"
if [[ -z "${KUDU_SIGN_IDENTITY:-}" ]] && security find-identity -v -p codesigning 2>/dev/null | grep -q '"Kudu Development"'; then
  KUDU_SIGN_IDENTITY="Kudu Development"
fi
IDENTITY="${KUDU_SIGN_IDENTITY:--}"
APP="build/Kudu.app"

swift build -c "$CONFIG" --arch arm64 --product Kudu
BIN_DIR="$(swift build -c "$CONFIG" --arch arm64 --show-bin-path)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/Kudu" "$APP/Contents/MacOS/Kudu"
cp Sources/Kudu/Info.plist "$APP/Contents/Info.plist"
cp Resources/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
# SwiftPM resource bundles from dependencies (unused by the ASR path, but keep them so
# Bundle.module lookups never crash).
find "$BIN_DIR" -maxdepth 1 -name '*.bundle' -exec cp -R {} "$APP/Contents/Resources/" \;

codesign --force --options runtime --timestamp=none \
  --entitlements Sources/Kudu/Kudu.entitlements \
  --sign "$IDENTITY" "$APP"

echo "Built $APP ($(du -sh "$APP" | cut -f1)), signed with: $IDENTITY"
if [[ "$IDENTITY" == "-" ]]; then
  echo "⚠ Ad-hoc signature: macOS will forget Kudu's Accessibility permission on every rebuild."
  echo "  Run scripts/setup-dev-signing.sh once to fix that."
fi
