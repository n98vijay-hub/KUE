#!/bin/bash
# Builds KueAct.app — the Action Broker's executor for apps, links and notifications.
# A bundle, not a bare tool: UserNotifications requires a bundle identity.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$HERE/bundle/KueAct.app"
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.lantern.act</string>
  <key>CFBundleName</key><string>Lantern</string>
  <key>CFBundleDisplayName</key><string>Lantern</string>
  <key>CFBundleExecutable</key><string>kue-act</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>26.0</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST
swiftc -target arm64-apple-macos26.0 -swift-version 5 -O -framework AppKit -framework UserNotifications \
  "$HERE/Sources/KueAct/"*.swift -o "$APP/Contents/MacOS/kue-act"
codesign --force --sign - --identifier dev.lantern.act "$APP"
echo "✓ built $APP"
