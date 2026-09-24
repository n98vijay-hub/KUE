#!/bin/bash
# Builds kue-auth: the LocalAuthentication (Touch ID / login password) helper.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$HERE/bin"
swiftc -target arm64-apple-macos26.0 -swift-version 5 -O -framework LocalAuthentication \
  "$HERE/Sources/KueAuth/"*.swift -o "$HERE/bin/kue-auth"
codesign --force --sign - --identifier dev.lantern.auth "$HERE/bin/kue-auth"
echo "✓ built $HERE/bin/kue-auth"
