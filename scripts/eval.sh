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
  python3 - "$@" <<'PY'
import json
import sys

mode, stream_log, min_fps, max_rtt = sys.argv[1], sys.argv[2], float(sys.argv[3]), float(sys.argv[4])
extra = sys.argv[5:]
lines = open(stream_log).read().splitlines()
summaries = [line.removeprefix("summary ") for line in lines if line.startswith("summary ")]
errors = []
stream = json.loads(summaries[-1]) if summaries else {}
if not stream:
    errors.append("stream printed no summary")
report = {"mode": mode, "stream": stream}
if mode == "mac":
    headset = json.load(open(extra[0])) if extra and extra[0] else {}
    report["headset"] = headset
    if not headset.get("passed"):
        errors += [f"headset: {e}" for e in headset.get("errors", ["no report"])]
    status = open(extra[1]).read().strip() if len(extra) > 1 else ""
    if "off" not in status.splitlines():
        errors.append(f"virtual screens left running after the stream: {status[:80]}")
else:
    log = open(extra[0]).read() if extra else ""
    report["headset_log_lines"] = len(log.splitlines())
    for needle in ["connected to the Mac", "screens:"]:
        if needle not in log:
            errors.append(f"headset log never said {needle!r}")
    for bad in ["fatal:", "decode:", "decoder:"]:
        hits = [line for line in log.splitlines() if bad in line]
        if hits:
            errors.append(f"headset log: {hits[0][-120:]}")
if stream:
    fps = stream.get("fps") or 0.0
    report["fps"] = fps
    if not stream.get("connected"):
        errors.append("the headset never connected")
    if fps < min_fps:
        errors.append(f"{fps:.1f} fps is below {min_fps:.0f}")
    if stream.get("round_trips", 0) < 10:
        errors.append(f"only {stream.get('round_trips', 0)} decoded-frame round trips")
    p50 = stream.get("round_trip_p50_ms")
    if p50 is None or p50 > max_rtt:
        errors.append(f"round trip p50 {p50} ms is above {max_rtt:.0f} ms")
report["errors"] = errors
report["passed"] = not errors
print(json.dumps(report, indent=2))
sys.exit(0 if not errors else 1)
PY
}

eval_mac() {
  build_mac
  cargo build --release -q -p fake-headset
  mac/scripts/run.sh stream --seconds "$((STREAM_SECONDS + 4))" --no-headset --test-pattern > "$OUT/stream.log" 2>&1 &
  local stream=$!
  target/release/fake-headset --seconds "$STREAM_SECONDS" --connect-timeout 20 > "$OUT/headset.json" || true
  wait "$stream" || true
  mac/scripts/run.sh status > "$OUT/status.txt" 2>&1 || true
  judge mac "$OUT/stream.log" "$MIN_FPS" "$MAX_ROUND_TRIP_MS" "$OUT/headset.json" "$OUT/status.txt" | tee "$OUT/mac.json"
}

eval_quest() {
  source scripts/android-env.sh
  [[ "$("$ADB" get-state 2>/dev/null || true)" == device ]] || fail "no Quest in adb devices; plug it in and allow USB debugging"
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
  "$ADB" exec-out screencap -p > "$OUT/quest.png" 2>/dev/null || rm -f "$OUT/quest.png"
  judge quest "$OUT/stream.log" "$MIN_FPS" "$MAX_ROUND_TRIP_MS" "$OUT/headset.log" | tee "$OUT/quest.json"
}

case "${1:-}" in
  mac) eval_mac ;;
  quest) eval_quest ;;
  *) usage ;;
esac
