#!/usr/bin/env python3
import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

MIN_ROUND_TRIPS = 10
HEADSET_MUST_SAY = ("connected to the Mac", "screens:")
HEADSET_MUST_NOT_SAY = ("fatal:", "decode:", "decoder:")
QUOTED_LOG_CHARS = 120
QUOTED_STATUS_CHARS = 80


@dataclass(frozen=True)
class Limits:
    min_fps: float
    max_round_trip_ms: float


def stream_summary(log: str) -> dict[str, Any]:
    summaries = [line.removeprefix("summary ") for line in log.splitlines() if line.startswith("summary ")]
    return json.loads(summaries[-1]) if summaries else {}


def stream_errors(stream: dict[str, Any], limits: Limits) -> list[str]:
    if not stream:
        return ["stream printed no summary"]
    errors = []
    fps = stream.get("fps") or 0.0
    if not stream.get("connected"):
        errors.append("the headset never connected")
    if fps < limits.min_fps:
        errors.append(f"{fps:.1f} fps is below {limits.min_fps:.0f}")
    round_trips = stream.get("round_trips", 0)
    if round_trips < MIN_ROUND_TRIPS:
        errors.append(f"only {round_trips} decoded-frame round trips")
    p50 = stream.get("round_trip_p50_ms")
    if p50 is None or p50 > limits.max_round_trip_ms:
        errors.append(f"round trip p50 {p50} ms is above {limits.max_round_trip_ms:.0f} ms")
    return errors


def mac_errors(headset: dict[str, Any], status: str) -> list[str]:
    errors = []
    if not headset.get("passed"):
        errors += [f"headset: {e}" for e in headset.get("errors", ["no report"])]
    if "off" not in status.splitlines():
        errors.append(f"virtual screens left running after the stream: {status[:QUOTED_STATUS_CHARS]}")
    return errors


def quest_errors(headset_log: str) -> list[str]:
    errors = [f"headset log never said {needle!r}" for needle in HEADSET_MUST_SAY if needle not in headset_log]
    for bad in HEADSET_MUST_NOT_SAY:
        hits = [line for line in headset_log.splitlines() if bad in line]
        if hits:
            errors.append(f"headset log: {hits[0][-QUOTED_LOG_CHARS:]}")
    return errors


def judge(mode: str, stream_log: str, limits: Limits, evidence: list[str]) -> dict[str, Any]:
    stream = stream_summary(stream_log)
    report: dict[str, Any] = {"mode": mode, "stream": stream, "fps": stream.get("fps") or 0.0}
    errors = stream_errors(stream, limits)
    if mode == "mac":
        headset = json.loads(evidence[0]) if evidence and evidence[0].strip() else {}
        report["headset"] = headset
        errors += mac_errors(headset, evidence[1] if len(evidence) > 1 else "")
    else:
        headset_log = evidence[0] if evidence else ""
        report["headset_log_lines"] = len(headset_log.splitlines())
        errors += quest_errors(headset_log)
    report["errors"] = errors
    report["passed"] = not errors
    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Judge an end-to-end eval run")
    parser.add_argument("mode", choices=["mac", "quest"])
    parser.add_argument("stream_log", type=Path)
    parser.add_argument("--min-fps", type=float, required=True)
    parser.add_argument("--max-round-trip-ms", type=float, required=True)
    parser.add_argument("evidence", nargs="*", type=Path)
    args = parser.parse_args(argv)
    evidence = [path.read_text() if path.exists() else "" for path in args.evidence]
    limits = Limits(min_fps=args.min_fps, max_round_trip_ms=args.max_round_trip_ms)
    report = judge(args.mode, args.stream_log.read_text(), limits, evidence)
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
