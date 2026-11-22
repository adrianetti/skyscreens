import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from swift_coverage import problems


class SwiftCoverageTests(unittest.TestCase):
    def test_pure_files_need_coverage(self) -> None:
        found = problems(["Layout.swift", "New.swift"], {"Layout.swift": 95.0, "New.swift": 40.0})
        self.assertEqual(len(found), 1)
        self.assertIn("New.swift: 40.0%", found[0])

    def test_files_missing_from_the_report_count_as_uncovered(self) -> None:
        self.assertEqual(len(problems(["Untested.swift"], {})), 1)

    def test_hardware_files_are_exempt(self) -> None:
        self.assertEqual(problems(["Streamer.swift"], {"Streamer.swift": 0.0}), [])


if __name__ == "__main__":
    unittest.main()
