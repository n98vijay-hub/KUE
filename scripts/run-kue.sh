#!/bin/bash
# Open KUE.
#
#   ./scripts/run-kue.sh           open the release build
#   ./scripts/run-kue.sh --build   build first, then open
#   ./scripts/run-kue.sh --debug   open (or with --build, build) the debug bundle
#
# Only one KUE can hold its local memory at a time, so a copy that is already
# running is asked to quit first — the ordinary way, as ⌘Q would. It is never
# force-killed: KUE's own kill switch is a different thing, and a quit is not it.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="release"; BUILD=0; BUILD_FLAGS=()
for arg in "$@"; do
  case "$arg" in
    --build) BUILD=1 ;;
    --debug) PROFILE="debug"; BUILD_FLAGS+=(--debug) ;;
    *) echo "usage: $0 [--build] [--debug]"; exit 2 ;;
  esac
done

if [[ $BUILD -eq 1 ]]; then
  ./scripts/build-kue.sh ${BUILD_FLAGS[@]+"${BUILD_FLAGS[@]}"}
fi

APP="$ROOT/target/$PROFILE/bundle/macos/KUE.app"
if [[ ! -d "$APP" ]]; then
  echo "✗ No $PROFILE build at $APP"
  echo "  Build it: ./scripts/run-kue.sh --build"
  exit 1
fi

# Any running copy — including one still named Lantern.app from before the rename.
if pgrep -f "(Lantern|KUE).app/Contents/MacOS/lantern$" >/dev/null; then
  echo "──▶ Asking the running KUE to quit"
  osascript -e 'tell application id "dev.lantern.desktop" to quit' >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do
    pgrep -f "(Lantern|KUE).app/Contents/MacOS/lantern$" >/dev/null || break
    sleep 0.5
  done
  if pgrep -fl "(Lantern|KUE).app/Contents/MacOS/lantern$"; then
    echo "✗ KUE is still running after 10 s. Quit it yourself (⌘Q), then run this again."
    exit 1
  fi
fi

echo "──▶ Opening $APP"
open "$APP"
