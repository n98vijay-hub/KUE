#!/bin/bash
# Builds the Lantern mind: the on-device language model process.
#
# A plain executable, not an .app: it requests no macOS permission (no camera,
# microphone or screen), so it needs no bundle identity for TCC.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/bin"
mkdir -p "$OUT"

echo "→ compiling Swift mind..."
swiftc \
  -target arm64-apple-macos26.0 \
  -swift-version 5 \
  -O \
  -framework FoundationModels \
  "$HERE/Sources/LanternMind/"*.swift \
  -o "$OUT/lantern-mind"

codesign --force --sign - --identifier dev.lantern.mind "$OUT/lantern-mind"
echo "✓ built $OUT/lantern-mind"
