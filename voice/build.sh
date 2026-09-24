#!/bin/bash
# Builds kue-voice: KUE's speech output process (AVSpeechSynthesizer).
#
# A plain executable, not an .app: speaking needs no macOS permission (no
# camera, microphone or screen), so it needs no bundle identity for TCC.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/bin"
mkdir -p "$OUT"

echo "→ compiling Swift voice output..."
swiftc \
  -target arm64-apple-macos26.0 \
  -swift-version 5 \
  -O \
  -framework AVFoundation \
  "$HERE/Sources/KueVoice/"*.swift \
  -o "$OUT/kue-voice"

codesign --force --sign - --identifier dev.lantern.voice "$OUT/kue-voice"
echo "✓ built $OUT/kue-voice"
