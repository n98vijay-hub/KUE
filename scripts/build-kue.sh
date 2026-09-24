#!/bin/bash
# Build KUE.app.
#
#   ./scripts/build-kue.sh           release build (what you normally want)
#   ./scripts/build-kue.sh --debug   faster to build, slower to run
#
# The bundle lands in target/<profile>/bundle/macos/KUE.app. Open it with
# ./scripts/run-kue.sh. Nothing here needs VS Code, Xcode's IDE or Tauri knowledge.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# A release build writes several GB; fail early and say why rather than halfway.
FREE_GB=$(df -g "$ROOT" | awk 'NR==2 {print $4}')
if [[ "${FREE_GB:-0}" -lt 8 ]]; then
  echo "✗ Only ${FREE_GB} GB free on this disk. A KUE build needs about 8 GB."
  echo "  'cargo clean' in $ROOT frees the previous build."
  exit 1
fi

command -v cargo >/dev/null || { echo "✗ Rust is not installed (https://rustup.rs)."; exit 1; }
command -v swiftc >/dev/null || { echo "✗ The Swift compiler is missing: xcode-select --install"; exit 1; }
[[ -d node_modules ]] || { echo "──▶ Installing front-end dependencies (first build only)"; npm ci; }

exec ./scripts/build-app.sh "$@"
