#!/bin/bash
# Builds the Lantern sensing layer into LanternSense.app
#
# Why an .app bundle and not a bare executable: macOS TCC (the camera permission
# system) will NOT display a permission prompt for an unbundled binary — it fails
# closed and returns "denied" instantly. A bundled, code-signed app has a stable
# identity TCC can attribute the request to. This was verified empirically.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-$HERE/bundle}"
APP="$OUT/LanternSense.app"
DEPLOY_TARGET="arm64-apple-macos26.0"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.lantern.sense</string>
  <key>CFBundleName</key><string>LanternSense</string>
  <key>CFBundleDisplayName</key><string>KUE Sensing</string>
  <key>CFBundleExecutable</key><string>lantern-sense</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>26.0</string>
  <key>LSUIElement</key><true/>
  <key>NSCameraUsageDescription</key>
  <string>KUE uses the camera locally to detect whether a face is present and to recognise you. Frames are analysed in memory and never recorded, stored, or sent anywhere.</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>KUE listens when you press Speak and, only if you turn it on, while it waits to hear its name. Speech is recognised on this Mac; audio is analysed in memory and never recorded, stored, or sent anywhere.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>KUE turns what you say after pressing Speak, or after saying its name if you turned that on, into text on this Mac. Nothing is sent anywhere.</string>
</dict>
PLIST
echo '</plist>' >> "$APP/Contents/Info.plist"

echo "→ compiling Swift sensing layer..."
swiftc \
  -target "$DEPLOY_TARGET" \
  -swift-version 5 \
  -O \
  -framework AVFoundation -framework Speech -framework Vision -framework AppKit -framework CoreImage -framework IOKit \
  "$HERE/Sources/LanternSense/"*.swift \
  -o "$APP/Contents/MacOS/lantern-sense"

echo "→ code signing (ad-hoc)..."
codesign --force --sign - --identifier dev.lantern.sense "$APP"

echo "✓ built $APP"
