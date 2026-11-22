import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from limits import MAX_LINES, MAX_LINES_RESEARCH, MAX_LINES_SHELL, TrackedFile, problems_in


def lines(count: int) -> str:
    return "x\n" * count


class LimitTests(unittest.TestCase):
    def test_code_files_have_a_line_limit(self) -> None:
        self.assertEqual(problems_in(TrackedFile("a.rs", "100644"), lines(MAX_LINES)), [])
        self.assertEqual(len(problems_in(TrackedFile("a.rs", "100644"), lines(MAX_LINES + 1))), 1)

    def test_research_and_shell_have_their_own_limits(self) -> None:
        self.assertEqual(problems_in(TrackedFile("bench/a.swift", "100644"), lines(MAX_LINES_RESEARCH)), [])
        long_script = "#!/usr/bin/env bash\nset -euo pipefail\n" + lines(MAX_LINES_SHELL)
        self.assertEqual(len(problems_in(TrackedFile("scripts/a.sh", "100755"), long_script)), 1)

    def test_docs_have_no_line_limit(self) -> None:
        self.assertEqual(problems_in(TrackedFile("docs/a.md", "100644"), lines(MAX_LINES * 3)), [])


class WhitespaceTests(unittest.TestCase):
    def test_clean_text_passes(self) -> None:
        self.assertEqual(problems_in(TrackedFile("a.py", "100644"), "x = 1\n"), [])

    def test_whitespace_problems_are_reported(self) -> None:
        found = problems_in(TrackedFile("a.py", "100644"), "x = 1 \n\ty = 2\r\n\n")
        self.assertEqual(len(found), 4, found)

    def test_markdown_may_end_lines_with_spaces(self) -> None:
        self.assertEqual(problems_in(TrackedFile("a.md", "100644"), "line  \n"), [])

    def test_missing_final_newline(self) -> None:
        self.assertEqual(problems_in(TrackedFile("a.toml", "100644"), "a = 1"), ["does not end with a newline"])


class ShellHeaderTests(unittest.TestCase):
    def test_executable_scripts_need_strict_mode(self) -> None:
        self.assertEqual(len(problems_in(TrackedFile("x/run.sh", "100755"), "#!/bin/sh\necho hi\n")), 1)
        self.assertEqual(problems_in(TrackedFile("x/run.sh", "100755"), "#!/usr/bin/env bash\nset -euo pipefail\n"), [])

    def test_sourced_scripts_are_not_executable_and_skip_the_header(self) -> None:
        self.assertEqual(problems_in(TrackedFile("x/env.sh", "100644"), "export A=1\n"), [])


if __name__ == "__main__":
    unittest.main()
