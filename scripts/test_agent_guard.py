import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from agent_guard import bash_verdict, decision, protected

ROOT = Path("/repo")


def kind(result: dict[str, object] | None) -> object:
    if result is None:
        return None
    output = result["hookSpecificOutput"]
    return output["permissionDecision"] if isinstance(output, dict) else None


class ProtectedPathTests(unittest.TestCase):
    def test_rule_files_and_gate_scripts_are_protected(self) -> None:
        for path in [
            "ruff.toml",
            "crates/link/Cargo.toml",
            "scripts/check.sh",
            ".github/workflows/ci.yml",
            "escape-hatches.json",
            "./clippy.toml",
            ".claude/settings.json",
        ]:
            self.assertTrue(protected(path), path)

    def test_product_code_is_not(self) -> None:
        for path in ["crates/link/src/sender.rs", "mac/Sources/SkyscreensKit/Streamer.swift", "README.md"]:
            self.assertFalse(protected(path), path)


class EditToolTests(unittest.TestCase):
    def test_editing_a_rule_file_asks_the_human(self) -> None:
        event = {"tool_name": "Edit", "tool_input": {"file_path": "/repo/ruff.toml"}}
        self.assertEqual(kind(decision(event, ROOT)), "ask")

    def test_editing_product_code_passes(self) -> None:
        event = {"tool_name": "Write", "tool_input": {"file_path": "/repo/xr/src/net.rs"}}
        self.assertIsNone(decision(event, ROOT))


class BashTests(unittest.TestCase):
    def test_writes_to_rule_files_are_blocked(self) -> None:
        for command in [
            "echo 'ignore = []' > ruff.toml",
            "sed -i '' 's/deny/allow/' Cargo.toml",
            "cat x | tee .swiftlint.yml",
            "rm scripts/limits.py",
            "python3 - <<'EOF'\nopen('clippy.toml', 'w').write('')\nEOF",
            "python3 scripts/escape_hatches.py --update",
        ]:
            self.assertIsNotNone(bash_verdict(command, ROOT), command)

    def test_reading_and_running_checks_pass(self) -> None:
        for command in [
            "scripts/check.sh all > target/check.log 2>&1",
            "python3 scripts/escape_hatches.py",
            "cat ruff.toml",
            "grep -n deny Cargo.toml",
            "cargo test --workspace",
            "sed -i '' 's/a/b/' crates/link/src/sender.rs",
        ]:
            self.assertIsNone(bash_verdict(command, ROOT), command)

    def test_absolute_paths_are_judged_relative_to_the_repo(self) -> None:
        root = Path("/home/dev/repo/.claude/worktrees/feature")
        self.assertIsNone(bash_verdict(f"cd {root} && rm -rf /tmp/scratch/fresh", root))
        self.assertIsNone(bash_verdict(f"cp {root}/crates/link/src/sender.rs /tmp/copy.rs", root))
        self.assertIsNotNone(bash_verdict(f"rm {root}/ruff.toml", root))
        self.assertIsNotNone(bash_verdict(f"echo x > {root}/scripts/check.sh", root))

    def test_files_outside_the_repo_are_not_protected(self) -> None:
        self.assertIsNone(bash_verdict("rm /tmp/other/Cargo.toml", ROOT))

    def test_bash_blocks_are_denials(self) -> None:
        event = {"tool_name": "Bash", "tool_input": {"command": "rm deny.toml"}}
        self.assertEqual(kind(decision(event, ROOT)), "deny")


if __name__ == "__main__":
    unittest.main()
