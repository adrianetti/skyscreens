#!/usr/bin/env python3
import json
import sys
from pathlib import Path

MIN_LINE_PERCENT = 90.0
SOURCES = "mac/Sources/SkyscreensKit/"
DRIVES_HARDWARE_OR_PROCESSES = {
    "Adb.swift",
    "FramePipeline.swift",
    "LinkHelper.swift",
    "MovingBar.swift",
    "Probe.swift",
    "ProbeRecorder.swift",
    "ScreenCapture.swift",
    "Session.swift",
    "Streamer.swift",
    "VirtualDisplaySet.swift",
}


def coverage_by_file(report: Path) -> dict[str, float]:
    data = json.loads(report.read_text())
    return {
        Path(entry["filename"]).name: float(entry["summary"]["lines"]["percent"])
        for entry in data["data"][0]["files"]
        if SOURCES in entry["filename"]
    }


def problems(sources: list[str], coverage: dict[str, float]) -> list[str]:
    found = []
    for name in sorted(sources):
        if name in DRIVES_HARDWARE_OR_PROCESSES:
            continue
        percent = coverage.get(name, 0.0)
        if percent < MIN_LINE_PERCENT:
            found.append(f"{SOURCES}{name}: {percent:.1f}% of lines covered, the minimum is {MIN_LINE_PERCENT:.0f}%")
    return found


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: swift_coverage.py <codecov json from swift test --show-codecov-path>", file=sys.stderr)
        return 2
    root = Path(__file__).resolve().parent.parent
    sources = [path.name for path in (root / SOURCES).glob("*.swift")]
    found = problems(sources, coverage_by_file(Path(argv[0])))
    for line in found:
        print(line)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
