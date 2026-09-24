#!/bin/bash
# Run KUE's tests and say plainly what passed.
#
#   ./scripts/test-kue.sh              core, shell and window tests (no hardware)
#   ./scripts/test-kue.sh --live       also the live tests that only read or tidy up
#                                      after themselves: storage measurement, the
#                                      Trash round trip on KUE's own files, and the
#                                      on-device model
#   ./scripts/test-kue.sh --live-all   also the live tests that take over the screen
#                                      and speakers: opening Chrome, Finder and voice
#
# "Passed" here means AUTOMATED VERIFIED. It is not LIVE VERIFIED unless a live
# test ran, and even then only for what that test observed.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LIVE=0
case "${1:-}" in
  "") ;;
  --live) LIVE=1 ;;
  --live-all) LIVE=2 ;;
  *) echo "usage: $0 [--live|--live-all]"; exit 2 ;;
esac

FAILED=()
summary() { grep -E "^test result:" | awk '{p+=$4; f+=$6; i+=$8} END {printf "%d passed, %d failed, %d ignored", p, f, i}'; }

echo "──▶ core (lantern-core)"
OUT=$(cargo test -p lantern-core 2>&1); CODE=$?
echo "    $(echo "$OUT" | summary)"
[[ $CODE -eq 0 ]] || { FAILED+=("core"); echo "$OUT" | grep -E "^test .* FAILED|panicked" | head -20; }

echo "──▶ shell (lantern) — includes the real sensing, speech and kill tests"
OUT=$(cargo test -p lantern 2>&1); CODE=$?
echo "    $(echo "$OUT" | summary)"
[[ $CODE -eq 0 ]] || { FAILED+=("shell"); echo "$OUT" | grep -E "^test .* FAILED|panicked" | head -20; }

echo "──▶ window (TypeScript + vitest)"
[[ -d node_modules ]] || npm ci >/dev/null
if npx tsc --noEmit >/dev/null 2>&1; then echo "    types ok"; else FAILED+=("types"); npx tsc --noEmit | head -20; fi
OUT=$(npx vitest run 2>&1); CODE=$?
echo "    $(echo "$OUT" | grep -E "^\s+Tests " | sed 's/^ *//')"
[[ $CODE -eq 0 ]] || { FAILED+=("window"); echo "$OUT" | tail -30; }

if [[ $LIVE -ge 1 ]]; then
  echo "──▶ live, on this Mac (reads only, or cleans up after itself)"
  for t in storage_live_measures_this_mac_and_the_window_gets_only_what_it_may_show \
           trash_live_moves_kues_own_files_to_the_real_trash_and_puts_them_back \
           ask_the_real_model \
           a_real_question_is_answered_on_device_from_the_cleared_context_only; do
    if cargo test -p lantern "$t" -- --ignored --exact "tests::$t" >/tmp/kue-live-$$.log 2>&1
    then echo "    ✓ $t"; else FAILED+=("live:$t"); echo "    ✗ $t"; grep -E "panicked|assert" /tmp/kue-live-$$.log | head -5; fi
  done
  rm -f /tmp/kue-live-$$.log
fi
if [[ $LIVE -ge 2 ]]; then
  echo "──▶ live, taking over the screen and speakers"
  for t in open_chrome_live_resolves_the_installed_app_and_the_executor_verifies_it \
           the_tampa_request_runs_on_this_macs_real_folders_finder_and_voice; do
    if cargo test -p lantern "$t" -- --ignored --exact "tests::$t" >/tmp/kue-live-$$.log 2>&1
    then echo "    ✓ $t"; else FAILED+=("live:$t"); echo "    ✗ $t"; grep -E "panicked|assert" /tmp/kue-live-$$.log | head -5; fi
  done
  rm -f /tmp/kue-live-$$.log
fi

echo
if [[ ${#FAILED[@]} -eq 0 ]]; then
  echo "✓ everything that ran passed"
else
  echo "✗ failed: ${FAILED[*]}"
  exit 1
fi
