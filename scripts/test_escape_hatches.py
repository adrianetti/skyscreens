import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from escape_hatches import Inventory, differences, fingerprint, hatches_in


class RustTests(unittest.TestCase):
    def test_unsafe_expect_and_discards_are_counted(self) -> None:
        code = (
            '#![expect(unsafe_code, reason = "C ABI")]\n'
            "fn f() { unsafe { g() }; let _ = h(); let _ignored = k(); }\n"
            '#[expect(clippy::too_many_lines, clippy::cast_possible_truncation, reason = "x")]\n'
        )
        found = hatches_in("a.rs", code)
        self.assertEqual(found["rust unsafe"], 1)
        self.assertEqual(found["rust discarded result"], 2)
        self.assertEqual(found["rust expect unsafe_code"], 1)
        self.assertEqual(found["rust expect clippy::cast_possible_truncation"], 1)

    def test_words_in_strings_do_not_count(self) -> None:
        code = 'const A: &str = "unsafe transmute"; let b = r#"let _ = unsafe"#; let c = \'u\';\n'
        self.assertEqual(hatches_in("a.rs", code), {})

    def test_lint_names_are_not_the_keyword(self) -> None:
        self.assertEqual(hatches_in("a.rs", "#![forbid(unsafe_code)]\n"), {})


class SwiftTests(unittest.TestCase):
    def test_swift_escape_hatches(self) -> None:
        code = (
            "final class A: @unchecked Sendable {}\n"
            "nonisolated(unsafe) var b = 1\n"
            "let c = unsafe f(&d)\n"
            "_ = try? g()\n"
            "MainActor.assumeIsolated { h() }\n"
            'let e = "unsafe try? _ = "\n'
        )
        found = hatches_in("a.swift", code)
        self.assertEqual(found["swift @unchecked Sendable"], 1)
        self.assertEqual(found["swift nonisolated(unsafe)"], 1)
        self.assertEqual(found["swift unsafe"], 1)
        self.assertEqual(found["swift try?"], 1)
        self.assertEqual(found["swift discarded result"], 1)
        self.assertEqual(found["swift MainActor.assumeIsolated"], 1)


class ShellAndPythonTests(unittest.TestCase):
    def test_shell_failures_that_are_swallowed(self) -> None:
        found = hatches_in("a.sh", "a || true\nb 2> /dev/null\n")
        self.assertEqual(found, {"shell ignored failure": 1, "shell silenced stderr": 1})
        self.assertEqual(hatches_in("scripts/hooks/pre-commit", "x || true\n")["shell ignored failure"], 1)

    def test_python_any_outside_strings(self) -> None:
        self.assertEqual(hatches_in("a.py", 'x: Any = cast(int, "Any")\n'), {"python Any": 1, "python cast": 1})


class LedgerTests(unittest.TestCase):
    def test_any_change_is_reported(self) -> None:
        before: Inventory = {"hatches": {"rust unsafe": {"a.rs": 1}}, "fingerprints": {"ruff.toml": "sha256:1"}}
        after: Inventory = {
            "hatches": {"rust unsafe": {"a.rs": 2, "b.rs": 1}},
            "fingerprints": {"ruff.toml": "sha256:2"},
        }
        self.assertEqual(
            differences(before, after),
            ["rust unsafe: a.rs 1 -> 2", "rust unsafe: b.rs 0 -> 1", "rule file changed: ruff.toml"],
        )
        self.assertEqual(differences(after, after), [])

    def test_fingerprints_ignore_pinned_action_bumps_and_versions(self) -> None:
        old = "uses: actions/checkout@" + "a" * 40 + "\n"
        new = "uses: actions/checkout@" + "b" * 40 + "\n"
        self.assertEqual(fingerprint(".github/workflows/ci.yml", old), fingerprint(".github/workflows/ci.yml", new))
        self.assertEqual(fingerprint("Cargo.toml", 'log = "0.4"\n'), fingerprint("Cargo.toml", 'log = "0.4.29"\n'))
        self.assertNotEqual(fingerprint("Cargo.toml", 'a = "allow"\n'), fingerprint("Cargo.toml", 'a = "deny"\n'))


if __name__ == "__main__":
    unittest.main()
