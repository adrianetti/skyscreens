import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from comments import comments_in, strip


def found(name: str, text: str) -> list[str]:
    return [text[c.start : c.end] for c in comments_in(Path(name), text)]


class RustTests(unittest.TestCase):
    def test_line_doc_and_block_comments(self) -> None:
        text = "//! crate\n/// item\nfn a() {} // tail\n/* outer /* nested */ still */\n"
        self.assertEqual(found("a.rs", text), ["//! crate", "/// item", "// tail", "/* outer /* nested */ still */"])

    def test_strings_chars_and_lifetimes_are_not_comments(self) -> None:
        text = (
            "let a = \"// no\"; let b = r#\"/* no */\"#; let c = '/'; let d = '\\'';\n"
            "fn f<'a>(x: &'a str) -> &'static str { \"\\\"//\" }\n"
            "let e = b'/'; let url = \"http://x\";\n"
        )
        self.assertEqual(found("a.rs", text), [])

    def test_raw_identifiers_are_code(self) -> None:
        self.assertEqual(found("a.rs", "let r#type = 1; // c\n"), ["// c"])


class SwiftTests(unittest.TestCase):
    def test_comments(self) -> None:
        self.assertEqual(found("a.swift", "let a = 1 // x\n/// doc\n/* b */\n"), ["// x", "/// doc", "/* b */"])

    def test_strings_with_interpolation_and_raw_strings(self) -> None:
        text = 'let a = "\\(b ? "//" : "/*")"\nlet c = #"// \\#(d)"#\nlet e = """\n// inside\n"""\n'
        self.assertEqual(found("a.swift", text), [])

    def test_tools_version_line_is_allowed(self) -> None:
        self.assertEqual(found("Package.swift", "// swift-tools-version: 5.10\nimport A // x\n"), ["// x"])


class ShellTests(unittest.TestCase):
    def test_comments_and_shebang(self) -> None:
        text = "#!/usr/bin/env bash\n# top\necho hi # tail\n"
        self.assertEqual(found("a.sh", text), ["# top", "# tail"])

    def test_hash_in_code_and_strings(self) -> None:
        text = 'echo "$#" \'${#x}\' "a # b" x#y ${#arr}\n'
        self.assertEqual(found("a.sh", text), [])

    def test_heredoc_bodies_are_data(self) -> None:
        text = "cat <<'EOF'\n# not a comment\nEOF\n# comment\n"
        self.assertEqual(found("a.sh", text), ["# comment"])


class PythonTests(unittest.TestCase):
    def test_comments_and_docstrings(self) -> None:
        text = '"""Module."""\nx = "# no"  # yes\ndef f():\n    """Doc."""\n    return 1\n'
        self.assertEqual(found("a.py", text), ['"""Module."""', "# yes", '"""Doc."""'])


class StripTests(unittest.TestCase):
    def test_full_line_comments_disappear_and_tails_are_trimmed(self) -> None:
        text = "// head\nfn a() {} // tail\n    // inner\nfn b() {}\n"
        self.assertEqual(strip(text, comments_in(Path("a.rs"), text)), "fn a() {}\nfn b() {}\n")

    def test_shebang_survives(self) -> None:
        text = "#!/bin/sh\n# c\necho 1\n"
        self.assertEqual(strip(text, comments_in(Path("a.sh"), text)), "#!/bin/sh\necho 1\n")


if __name__ == "__main__":
    unittest.main()
