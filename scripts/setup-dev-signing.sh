#!/usr/bin/env bash
# One-time setup: creates a self-signed "Kudu Development" code-signing certificate in your
# login keychain. scripts/build-app.sh uses it automatically.
#
# Why: macOS remembers Accessibility/Microphone permission per code signature. Ad-hoc
# signatures change on every build, so the permission you granted stops matching and Kudu
# asks again. A stable certificate keeps the same signature across rebuilds.
#
# macOS will ask for your login password once, to trust the certificate for code signing.
set -euo pipefail

NAME="Kudu Development"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "\"$NAME\""; then
  echo "✓ \"$NAME\" already exists. Nothing to do."
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PASS="kudu-$(uuidgen)"

cat > "$WORK/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$WORK/key.pem" -out "$WORK/cert.pem" -config "$WORK/cert.cnf" 2>/dev/null

# macOS's importer needs the legacy PKCS#12 encryption.
openssl pkcs12 -export -legacy -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
  -name "$NAME" -out "$WORK/cert.p12" -passout "pass:$PASS"

security import "$WORK/cert.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null
echo "Trusting the certificate for code signing (macOS will ask for your password)…"
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$WORK/cert.pem"

if security find-identity -v -p codesigning | grep -q "\"$NAME\""; then
  echo "✓ Created \"$NAME\". Rebuild with scripts/build-app.sh."
else
  echo "✗ The certificate was imported but isn't valid for code signing yet." >&2
  exit 1
fi
