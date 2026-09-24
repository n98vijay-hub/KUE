#!/bin/bash
# Builds KUE.app, with the Swift sensing layer embedded inside it.
#
# The bundle was called Lantern.app until 2026-09-16. Only the bundle's file
# name changed: the identifier (dev.lantern.desktop), the helpers' identifiers
# and the data folder (~/Library/Application Support/Lantern) did not, so macOS
# permissions and local memory carry over exactly as they do across any rebuild.
# See docs/KUE_RENAME_PLAN.md.
#
#   ./scripts/build-app.sh           release build
#   ./scripts/build-app.sh --debug   faster build, for iteration
#
# KUE MUST be run as a bundled .app. macOS will not show a camera
# permission prompt for an unbundled binary — it fails closed and reports
# "denied" with no dialog. `tauri dev` runs an unbundled binary, so the camera
# will not work there. Use this script and open the .app.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="release"
TAURI_FLAGS=()
if [[ "${1:-}" == "--debug" ]]; then
  PROFILE="debug"
  TAURI_FLAGS+=(--debug)
fi

echo "──▶ 1/4  Swift sensing layer"
./sensing/build.sh >/dev/null
echo "    ✓ LanternSense.app"

./auth/build.sh >/dev/null
./mind/build.sh >/dev/null
./act/build.sh >/dev/null
./voice/build.sh >/dev/null
echo "    ✓ KueAct.app (Action Broker executor)"
echo "    ✓ kue-voice (speech output, AVSpeechSynthesizer)"
echo "    ✓ lantern-mind (on-device language model process)"
echo "    ✓ kue-auth (LocalAuthentication helper)"

echo "──▶ 2/4  Tauri app ($PROFILE)"
npx tauri build ${TAURI_FLAGS[@]+"${TAURI_FLAGS[@]}"} --bundles app 2>&1 | grep -viE "^\s+(Compiling|Downloaded|Adding)" | tail -12

APP="$ROOT/target/$PROFILE/bundle/macos/KUE.app"
[[ -d "$APP" ]] || { echo "✗ expected bundle at $APP"; exit 1; }

echo "──▶ 3/4  Embedding the sensing layer"
# Copied with ditto so the nested bundle's code signature and permissions survive.
rm -rf "$APP/Contents/Resources/LanternSense.app"
ditto "$ROOT/sensing/bundle/LanternSense.app" "$APP/Contents/Resources/LanternSense.app"

# The Touch ID / password helper sits beside the main executable.
cp "$ROOT/auth/bin/kue-auth" "$APP/Contents/MacOS/kue-auth"
codesign --force --sign - --identifier dev.lantern.auth "$APP/Contents/MacOS/kue-auth"
cp "$ROOT/voice/bin/kue-voice" "$APP/Contents/MacOS/kue-voice"
codesign --force --sign - --identifier dev.lantern.voice "$APP/Contents/MacOS/kue-voice"
cp "$ROOT/mind/bin/lantern-mind" "$APP/Contents/MacOS/lantern-mind"
codesign --force --sign - --identifier dev.lantern.mind "$APP/Contents/MacOS/lantern-mind"

rm -rf "$APP/Contents/Resources/KueAct.app"
ditto "$ROOT/act/bundle/KueAct.app" "$APP/Contents/Resources/KueAct.app"
codesign --force --sign - --identifier dev.lantern.act "$APP/Contents/Resources/KueAct.app"

# The camera usage string must be present in the OUTER bundle too: TCC attributes
# the helper's request to its responsible process, which is KUE itself.
PLIST="$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Print :NSCameraUsageDescription" "$PLIST" >/dev/null 2>&1 || \
  /usr/libexec/PlistBuddy -c "Add :NSCameraUsageDescription string 'KUE uses the camera locally to detect whether a face is present and to recognise you. Frames are analysed in memory and are never recorded, stored, or sent anywhere.'" "$PLIST"
/usr/libexec/PlistBuddy -c "Print :NSMicrophoneUsageDescription" "$PLIST" >/dev/null 2>&1 || \
  /usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'KUE listens when you press Speak and, only if you turn it on, while it waits to hear its name. Speech is recognised on this Mac; audio is analysed in memory and never recorded, stored, or sent anywhere.'" "$PLIST"
for folder in Desktop Documents Downloads; do
  /usr/libexec/PlistBuddy -c "Print :NS${folder}FolderUsageDescription" "$PLIST" >/dev/null 2>&1 || \
    /usr/libexec/PlistBuddy -c "Add :NS${folder}FolderUsageDescription string 'KUE looks in your ${folder} folder only when you ask it to open a document by name, and opens a file only after you confirm it. Nothing is changed, stored or sent anywhere.'" "$PLIST"
done
/usr/libexec/PlistBuddy -c "Print :NSSpeechRecognitionUsageDescription" "$PLIST" >/dev/null 2>&1 || \
  /usr/libexec/PlistBuddy -c "Add :NSSpeechRecognitionUsageDescription string 'KUE turns what you say after pressing Speak, or after saying its name if you turned that on, into text on this Mac. Nothing is sent anywhere.'" "$PLIST"
echo "    ✓ NSCameraUsageDescription: $(/usr/libexec/PlistBuddy -c "Print :NSCameraUsageDescription" "$PLIST" | cut -c1-54)…"

echo "──▶ 4/4  Signing"
# Inside-out: the nested helper first, then the outer bundle.
codesign --force --sign - --identifier dev.lantern.sense "$APP/Contents/Resources/LanternSense.app"
codesign --force --sign - --identifier dev.lantern.desktop "$APP"
codesign --verify --deep "$APP" && echo "    ✓ signature verifies"

echo
echo "✓ $APP"
echo "  open \"$APP\""

# Debug and release bundles live at different paths, so a copy of either may
# still be running. Only one instance can hold local memory; a second one reports
# STORAGE_UNAVAILABLE. Quit the old one before opening the new build.
if RUNNING=$(pgrep -fl "(Lantern|KUE).app/Contents/MacOS/lantern$"); then
  echo
  echo "⚠ KUE is already running — quit it before opening this build (./scripts/run-kue.sh does that):"
  echo "$RUNNING" | sed 's/^/    /'
fi
