#!/bin/zsh
# Runs a skyscreens command through Skyscreens.app and prints its output here.
# Going through the bundle matters: macOS checks Screen Recording for the app, not for this terminal.
#   mac/scripts/run.sh permission | status | on | off | probe [hz] | stream
# Ctrl-C stops the command; `stream` then removes its virtual screen.
set -euo pipefail
cd "$(dirname "$0")/.."
APP="$PWD/.build/Skyscreens.app"
BIN="$APP/Contents/MacOS/skyscreens"
[[ -x "$BIN" ]] || { echo "Build the app first: mac/scripts/bundle.sh" >&2; exit 1; }

LOG=$(mktemp -t skyscreens)
open -W -n --stdout "$LOG" --stderr "$LOG" "$APP" --args "$@" &
OPEN=$!
tail -n +1 -f "$LOG" &
TAIL=$!
trap 'pkill -INT -f "^$BIN" 2>/dev/null || true' INT TERM
while kill -0 $OPEN 2>/dev/null; do wait $OPEN 2>/dev/null || true; done
sleep 0.2
kill $TAIL 2>/dev/null || true
rm -f "$LOG"
