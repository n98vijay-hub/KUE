#!/bin/bash
# Back up KUE's source — every branch and tag, with full history — as one file.
#
#   ./scripts/backup-kue.sh <folder>
#
# Writes <folder>/KUE-<date>-<commit>.bundle and checks that git can read it.
# Restore anywhere with:  git clone <that file> KUE
#
# Only what is committed. It contains no build output, and none of KUE's local
# data — the enrollment, memory database and kill latch in
# ~/Library/Application Support/Lantern stay on this Mac and are never copied
# here. Nothing is uploaded: where the file goes is the folder you name.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEST="${1:-}"
[[ -n "$DEST" ]] || { echo "usage: $0 <folder to write the backup into>"; exit 2; }
[[ -d "$DEST" ]] || { echo "✗ $DEST is not a folder."; exit 1; }
DEST="$(cd "$DEST" && pwd)"
case "$DEST/" in
  "$ROOT/"*) echo "✗ $DEST is inside the repository. Choose a folder outside it."; exit 1 ;;
esac

if [[ -n "$(git status --porcelain)" ]]; then
  echo "⚠ There are uncommitted changes. They are NOT in this backup — commit them first if they matter."
fi

FILE="$DEST/KUE-$(date +%Y-%m-%d)-$(git rev-parse --short HEAD).bundle"
git bundle create "$FILE" --all
git bundle verify "$FILE" >/dev/null
echo "✓ $FILE"
echo "  $(du -h "$FILE" | cut -f1), $(git branch | wc -l | tr -d ' ') branches, $(git tag | wc -l | tr -d ' ') tags"
echo "  Restore with: git clone \"$FILE\" KUE"
