#!/bin/bash
# What KUE measured about itself. Read-only, counts and categories only.
#
#   ./scripts/evidence-kue.sh            the last hour
#   ./scripts/evidence-kue.sh 1800       the last 30 minutes
#   ./scripts/evidence-kue.sh 3600 out.txt   …and save it
#
# Run this after a seated session to turn "KUE felt unreliable" into numbers.
# Nothing here reads a transcript, a file name or anything about who was seen:
# the tables it queries cannot hold those.
set -uo pipefail
DB="$HOME/Library/Application Support/Lantern/lantern.sqlite3"
WINDOW="${1:-3600}"
OUT="${2:-}"
[[ -f "$DB" ]] || { echo "No local memory at $DB — has KUE ever run on this account?"; exit 1; }

q() { sqlite3 -readonly "$DB" "$1"; }
SINCE="strftime('%s','now') - $WINDOW"

report() {
  echo "KUE evidence — the last $((WINDOW / 60)) minutes, $(date '+%Y-%m-%d %H:%M')"
  echo

  echo "ACCESS — how often the owner's session changed"
  q "SELECT '  ' || COUNT(*) || ' changes, ' ||
            COALESCE(ROUND(COUNT(*) * 3600.0 / $WINDOW), 0) || ' per hour at this rate'
       FROM events WHERE kind='AccessChanged' AND ts > $SINCE;"
  q "SELECT '  ' || summary || '  x' || COUNT(*) FROM events
      WHERE kind='AccessChanged' AND ts > $SINCE GROUP BY summary ORDER BY COUNT(*) DESC;"
  echo
  echo "MEASUREMENT — how the measuring went"
  q "SELECT '  ' || measurement || '  x' || COUNT(*) ||
            '   age avg ' || COALESCE(ROUND(AVG(measurement_age_ms)),0) || ' ms, worst ' ||
            COALESCE(MAX(measurement_age_ms),0) || ' ms'
       FROM perception_samples WHERE ts > $SINCE GROUP BY measurement ORDER BY COUNT(*) DESC;"
  echo
  echo "  Of the samples with no current measurement, how many were a live pipeline being late?"
  q "SELECT '  delayed (pipeline proven alive): ' || SUM(measurement='MEASUREMENT_DELAYED') ||
            '   stale (nothing proves one is coming): ' || SUM(measurement='MEASUREMENT_STALE')
       FROM perception_samples WHERE ts > $SINCE;"
  echo
  echo "IDENTITY vs MEASUREMENT — what KUE concluded, against how well it could see"
  q "SELECT '  ' || identity || ' / ' || measurement || ' / ' || access_level || '  x' || n FROM (
        SELECT identity, measurement, access_level, COUNT(*) n
          FROM perception_samples WHERE ts > $SINCE
         GROUP BY identity, measurement, access_level ORDER BY n DESC LIMIT 12);"
  echo
  echo "STALLS — every moment the measurement crossed the staleness limit"
  q "SELECT '  ' || datetime(ts,'unixepoch','localtime') || '  age ' || measurement_age_ms ||
            ' ms · analysis ' || COALESCE(analyze_ms,0) || ' ms · capture gap ' ||
            COALESCE(capture_gap_ms,0) || ' ms · model ' || model_phase || ' · ' || measurement
       FROM perception_samples WHERE ts > $SINCE AND measurement_age_ms > 2500
       ORDER BY ts DESC LIMIT 20;"
  echo
  echo "STAGES — where the time went (only stages slow enough or rare enough to keep)"
  q "SELECT '  ' || stage || '  n=' || COUNT(*) || '  avg ' || ROUND(AVG(duration_ms)) ||
            ' ms  worst ' || MAX(duration_ms) || ' ms'
       FROM stage_timings WHERE ts > $SINCE GROUP BY stage ORDER BY MAX(duration_ms) DESC;"
  echo
  echo "MODEL — answers and how long they took"
  q "SELECT '  ' || datetime(ts,'unixepoch','localtime') || '  ' || summary
       FROM events WHERE kind='ModelInteraction' AND ts > $SINCE ORDER BY ts DESC LIMIT 10;"
  echo
  echo "LOCAL MEMORY — what it costs"
  q "SELECT '  events ' || (SELECT COUNT(*) FROM events) ||
            ' · snapshots ' || (SELECT COUNT(*) FROM context_snapshots) ||
            ' · perception samples ' || (SELECT COUNT(*) FROM perception_samples) ||
            ' · stage timings ' || (SELECT COUNT(*) FROM stage_timings);"
  echo "  file $(du -h "$DB" | cut -f1), of which $(q "SELECT ROUND((SELECT * FROM pragma_freelist_count) * (SELECT * FROM pragma_page_size) / 1048576.0, 1);") MB is free space it will reuse"
  echo
  echo "PRIVACY — what the firewall refused, all time"
  q "SELECT '  ' || kind || ' → ' || destination || '  ' || decision || '  x' || count
       FROM privacy_ledger WHERE decision='DENY' ORDER BY count DESC LIMIT 8;"
}

if [[ -n "$OUT" ]]; then report | tee "$OUT"; else report; fi
