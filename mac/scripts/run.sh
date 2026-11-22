#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
APP="$PWD/.build/Skyscreens.app"
BIN="$APP/Contents/MacOS/skyscreens"
[[ -x "$BIN" ]] || {
  echo "Build the app first: mac/scripts/bundle.sh" >&2
  exit 1
}

LOG=$(mktemp -t skyscreens)
open -W -n --stdout "$LOG" --stderr "$LOG" "$APP" --args "$@" 2> /dev/null &
OPEN=$!
tail -n +1 -f "$LOG" &
TAIL=$!
trap 'pkill -INT -f "^$BIN" 2>/dev/null || true' INT TERM
while kill -0 "$OPEN" 2> /dev/null; do wait "$OPEN" 2> /dev/null || true; done
sleep 0.2
kill "$TAIL" 2> /dev/null || true
rm -f "$LOG"
