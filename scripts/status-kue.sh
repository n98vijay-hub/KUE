#!/bin/bash
# Where KUE stands on this Mac: the code, the build, whether it is running.
# Reads only. It never opens KUE's memory — it only says whether the folder exists.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "CODE"
BRANCH=$(git rev-parse --abbrev-ref HEAD)
echo "  branch      $BRANCH @ $(git log -1 --format='%h %s' | cut -c1-70)"
echo "  committed   $(git log -1 --format=%cd --date=format:'%Y-%m-%d %H:%M')"
DIRTY=$(git status --porcelain | wc -l | tr -d ' ')
[[ "$DIRTY" == "0" ]] && echo "  changes     none uncommitted" || echo "  changes     $DIRTY file(s) not committed"
if git rev-parse --verify -q main >/dev/null; then
  echo "  vs main     $(git rev-list --count main..HEAD) ahead, $(git rev-list --count HEAD..main) behind"
fi

echo
echo "BUILD"
HEAD_TS=$(git log -1 --format=%ct)
for profile in release debug; do
  APP="$ROOT/target/$profile/bundle/macos/KUE.app"
  [[ -d "$APP" ]] || APP="$ROOT/target/$profile/bundle/macos/Lantern.app"
  if [[ -d "$APP" ]]; then
    BIN="$APP/Contents/MacOS/lantern"
    BUILT=$(stat -f %m "$BIN" 2>/dev/null || echo 0)
    WHEN=$(date -r "$BUILT" '+%Y-%m-%d %H:%M')
    if [[ "$BUILT" -ge "$HEAD_TS" ]]; then FRESH="built after the last commit"; else FRESH="OLDER than the last commit — rebuild"; fi
    SIG=$(codesign --verify --deep "$APP" 2>/dev/null && echo "signature verifies" || echo "SIGNATURE DOES NOT VERIFY")
    printf "  %-11s %s · %s · %s · %s\n" "$profile" "$(basename "$APP")" "$WHEN" "$FRESH" "$SIG"
  else
    printf "  %-11s not built\n" "$profile"
  fi
done

echo
echo "RUNNING"
if RUN=$(pgrep -fl "(Lantern|KUE).app/Contents/MacOS/lantern$"); then
  echo "$RUN" | sed 's/^/  /'
  for helper in LanternSense kue-voice lantern-mind; do
    pgrep -f "$helper" >/dev/null && echo "  helper      $helper running" || echo "  helper      $helper not running"
  done
else
  echo "  KUE is not running"
fi

echo
echo "LOCAL DATA"
DATA="$HOME/Library/Application Support/Lantern"
if [[ -d "$DATA" ]]; then
  echo "  folder      $DATA ($(du -sh "$DATA" 2>/dev/null | cut -f1))"
  [[ -e "$DATA/KILLED" ]] && echo "  kill switch ENGAGED — KUE starts stopped until you recover it in the app" \
                          || echo "  kill switch not engaged"
else
  echo "  folder      none yet (KUE has not run on this account)"
fi

echo
echo "DISK"
echo "  free        $(df -h "$ROOT" | awk 'NR==2 {print $4}') · build output $(du -sh "$ROOT/target" 2>/dev/null | cut -f1)"
