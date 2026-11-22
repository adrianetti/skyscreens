#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

OUT="$ROOT/target/eval"
STREAM_SECONDS="${EVAL_SECONDS:-8}"
MIN_FPS="${EVAL_MIN_FPS:-30}"
MAX_ROUND_TRIP_MS="${EVAL_MAX_ROUND_TRIP_MS:-60}"
QUEST_APP=io.github.adrianetti.skyscreens
mkdir -p "$OUT"

usage() {
  echo "usage: scripts/eval.sh mac | quest" >&2
  echo "  mac    Mac pipeline end to end against a fake headset (needs Screen Recording)" >&2
  echo "  quest  full pipeline on a Quest connected over USB" >&2
  exit 2
}

fail() {
  echo "eval: $*" >&2
  exit 1
}

build_mac() {
  if [[ "${EVAL_SKIP_BUILD:-0}" != 1 ]]; then
    mac/scripts/bundle.sh > "$OUT/bundle.log" 2>&1 || fail "Mac build failed, see $OUT/bundle.log"
  fi
  mac/scripts/run.sh permission > "$OUT/permission.txt" 2>&1 \
    || fail "Screen Recording is not granted to this build: $(head -1 "$OUT/permission.txt"). See docs/development.md#code-signing"
  mac/scripts/run.sh off > /dev/null 2>&1 || true
}

judge() {
  local mode=$1 log=$2
  shift 2
  python3 scripts/eval_judge.py "$mode" "$log" --min-fps "$MIN_FPS" --max-round-trip-ms "$MAX_ROUND_TRIP_MS" "$@"
}

eval_mac() {
  build_mac
  cargo build --release -q -p fake-headset
  mac/scripts/run.sh stream --seconds "$((STREAM_SECONDS + 4))" --no-headset --test-pattern > "$OUT/stream.log" 2>&1 &
  local stream=$!
  target/release/fake-headset --seconds "$STREAM_SECONDS" --connect-timeout 20 > "$OUT/headset.json" || true
  wait "$stream" || true
  mac/scripts/run.sh status > "$OUT/status.txt" 2>&1 || true
  judge mac "$OUT/stream.log" "$OUT/headset.json" "$OUT/status.txt" | tee "$OUT/mac.json"
}

eval_quest() {
  source scripts/android-env.sh
  [[ "$("$ADB" get-state 2> /dev/null || true)" == device ]] || fail "no Quest in adb devices; plug it in and allow USB debugging"
  build_mac
  if [[ "${EVAL_SKIP_BUILD:-0}" != 1 ]]; then
    xr/build.sh --install > "$OUT/apk.log" 2>&1 || fail "APK build or install failed, see $OUT/apk.log"
  fi
  "$ADB" shell am broadcast -a com.oculus.vrpowermanager.prox_close > /dev/null
  trap '"$ADB" shell am broadcast -a com.oculus.vrpowermanager.automation_disable > /dev/null 2>&1 || true' EXIT
  "$ADB" shell am force-stop "$QUEST_APP"
  "$ADB" logcat -c
  mac/scripts/run.sh stream --seconds "$((STREAM_SECONDS + 6))" --test-pattern > "$OUT/stream.log" 2>&1 || true
  "$ADB" logcat -d -s skyscreens:V > "$OUT/headset.log" 2>&1 || true
  "$ADB" exec-out screencap -p > "$OUT/quest.png" 2> /dev/null || rm -f "$OUT/quest.png"
  judge quest "$OUT/stream.log" "$OUT/headset.log" | tee "$OUT/quest.json"
}

case "${1:-}" in
  mac) eval_mac ;;
  quest) eval_quest ;;
  *) usage ;;
esac
