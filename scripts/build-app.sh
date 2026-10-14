#!/usr/bin/env bash
# Builds Kudu.app from the Swift package. No Xcode project needed.
#
#   scripts/build-app.sh            # release build → build/Kudu.app
#   CONFIG=debug scripts/build-app.sh
#
# Signing: ad-hoc by default. macOS ties Accessibility/Microphone grants to the signature,
# so ad-hoc builds have to be re-approved after every rebuild. To avoid that while
# developing, sign with a stable identity:
#   KUDU_SIGN_IDENTITY="Your Code Signing Cert" scripts/build-app.sh
set -euo pipefail

cd "$(dirname "$0")/.."
CONFIG="${CONFIG:-release}"
IDENTITY="${KUDU_SIGN_IDENTITY:--}"
APP="build/Kudu.app"

swift build -c "$CONFIG" --arch arm64 --product Kudu
BIN_DIR="$(swift build -c "$CONFIG" --arch arm64 --show-bin-path)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/Kudu" "$APP/Contents/MacOS/Kudu"
cp Sources/Kudu/Info.plist "$APP/Contents/Info.plist"
# SwiftPM resource bundles from dependencies (unused by the ASR path, but keep them so
# Bundle.module lookups never crash).
find "$BIN_DIR" -maxdepth 1 -name '*.bundle' -exec cp -R {} "$APP/Contents/Resources/" \;

codesign --force --options runtime --timestamp=none \
  --entitlements Sources/Kudu/Kudu.entitlements \
  --sign "$IDENTITY" "$APP"

echo "Built $APP ($(du -sh "$APP" | cut -f1)), signed with: $IDENTITY"
