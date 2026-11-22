import json
import sys
import unittest
from collections.abc import Mapping
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from eval_judge import Limits, judge

LIMITS = Limits(min_fps=30, max_round_trip_ms=60)
GOOD_STREAM = {"connected": True, "fps": 57.3, "round_trips": 452, "round_trip_p50_ms": 8.8}
GOOD_HEADSET = {"passed": True, "errors": []}


def stream_log(summary: Mapping[str, object]) -> str:
    return "streaming skyscreens 1\nconnected  57 fps\nsummary " + json.dumps(summary) + "\n"


class MacEvalTests(unittest.TestCase):
    def test_a_healthy_run_passes(self) -> None:
        report = judge("mac", stream_log(GOOD_STREAM), LIMITS, [json.dumps(GOOD_HEADSET), "off\n"])
        self.assertTrue(report["passed"], report["errors"])
        self.assertEqual(report["fps"], 57.3)

    def test_every_threshold_is_enforced(self) -> None:
        slow = {"connected": False, "fps": 12.0, "round_trips": 3, "round_trip_p50_ms": 90.0}
        report = judge("mac", stream_log(slow), LIMITS, [json.dumps(GOOD_HEADSET), "off"])
        self.assertEqual(len(report["errors"]), 4, report["errors"])

    def test_headset_errors_and_leftover_screens_fail_the_run(self) -> None:
        headset = {"passed": False, "errors": ["first frame is not a keyframe"]}
        report = judge("mac", stream_log(GOOD_STREAM), LIMITS, [json.dumps(headset), '{"pid": 42}'])
        self.assertIn("headset: first frame is not a keyframe", report["errors"])
        self.assertTrue(any("left running" in e for e in report["errors"]))

    def test_a_missing_summary_fails(self) -> None:
        report = judge("mac", "capture failed\n", LIMITS, ["", "off"])
        self.assertIn("stream printed no summary", report["errors"])
        self.assertIn("headset: no report", report["errors"])

    def test_the_last_summary_wins(self) -> None:
        log = stream_log({"fps": 1}) + stream_log(GOOD_STREAM)
        self.assertTrue(judge("mac", log, LIMITS, [json.dumps(GOOD_HEADSET), "off"])["passed"])


class QuestEvalTests(unittest.TestCase):
    def test_a_clean_headset_log_passes(self) -> None:
        log = "I skyscreens: connected to the Mac\nI skyscreens: screens: [Screen { id: 0 }]\n"
        self.assertTrue(judge("quest", stream_log(GOOD_STREAM), LIMITS, [log])["passed"])

    def test_decoder_errors_and_silence_fail(self) -> None:
        log = "E skyscreens: decoder: configure failed\n"
        errors = judge("quest", stream_log(GOOD_STREAM), LIMITS, [log])["errors"]
        self.assertTrue(any("never said 'connected to the Mac'" in e for e in errors))
        self.assertTrue(any("decoder: configure failed" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
