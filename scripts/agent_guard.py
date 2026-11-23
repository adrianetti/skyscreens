#!/usr/bin/env python3
import json
import re
import sys
from collections.abc import Mapping
from pathlib import Path

PROTECTED_FILES = {
    "escape-hatches.json",
    "AGENTS.md",
    "docs/code-standards.md",
    "clippy.toml",
    "rustfmt.toml",
    "rust-toolchain.toml",
    "deny.toml",
    "ruff.toml",
    "mypy.ini",
    ".swiftlint.yml",
    ".swift-format",
    "bench/.swift-format",
    ".shellcheckrc",
    ".editorconfig",
    "_typos.toml",
    "mac/Package.swift",
}
PROTECTED_DIRS = ("scripts/", ".github/", ".claude/")
PROTECTED_NAMES = ("Cargo.toml",)
EDIT_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
REDIRECT = re.compile(r"(?:>>?|\btee\s+(?:-a\s+)?)\s*([^\s;&|<>]+)")
WRITING_COMMAND = re.compile(
    r"\b(sed\s+-i|perl\s+-[a-z]*i|mv|cp|rm|truncate|chmod|ln|git\s+(?:checkout|restore|rm|mv))\b"
)
INLINE_PROGRAM = re.compile(r"<<|\s-c\s|\s-e\s")
WRITE_CALL = re.compile(r"write|open\(|replace\(|unlink|rename|rmtree|remove\(")
LEDGER_UPDATE = re.compile(r"escape_hatches\.py\b[^;&|]*--update")
TOKEN = re.compile(r"[\w./-]+")


def protected(path: str) -> bool:
    path = path.removeprefix("./")
    return path in PROTECTED_FILES or path.startswith(PROTECTED_DIRS) or Path(path).name in PROTECTED_NAMES


def repo_path(path: str, root: Path) -> str | None:
    candidate = Path(path)
    if not candidate.is_absolute():
        return path
    try:
        return str(candidate.resolve().relative_to(root.resolve()))
    except ValueError:
        return None


def protected_in_repo(path: str, root: Path) -> bool:
    inside = repo_path(path, root)
    return inside is not None and protected(inside)


def mentions_protected(command: str, root: Path) -> bool:
    tokens = [token for token in TOKEN.findall(command) if "/" in token or "." in token]
    return any(protected_in_repo(token, root) for token in tokens)


def bash_verdict(command: str, root: Path) -> str | None:
    if LEDGER_UPDATE.search(command):
        return "Only a human updates escape-hatches.json. Fix the code instead, or ask the maintainer."
    if any(protected_in_repo(target, root) for target in REDIRECT.findall(command)):
        return "This command writes a protected rule file. Use the Edit tool so the maintainer can approve it."
    if WRITING_COMMAND.search(command) and mentions_protected(command, root):
        return "This command changes a protected rule file. Use the Edit tool so the maintainer can approve it."
    if INLINE_PROGRAM.search(command) and WRITE_CALL.search(command) and mentions_protected(command, root):
        return "This inline program writes a protected rule file. Use the Edit tool so the maintainer can approve it."
    return None


def decision(event: Mapping[str, object], root: Path) -> dict[str, object] | None:
    tool = event.get("tool_name")
    tool_input = event.get("tool_input")
    if not isinstance(tool_input, dict):
        return None
    if tool in EDIT_TOOLS:
        path = tool_input.get("file_path") or tool_input.get("notebook_path")
        if isinstance(path, str) and protected_in_repo(path, root):
            return verdict("ask", f"{repo_path(path, root)} is a protected rule file; the maintainer approves changes.")
    if tool == "Bash":
        command = tool_input.get("command")
        reason = bash_verdict(command, root) if isinstance(command, str) else None
        if reason:
            return verdict("deny", reason)
    return None


def verdict(kind: str, reason: str) -> dict[str, object]:
    return {
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": kind,
            "permissionDecisionReason": reason,
        }
    }


def main() -> int:
    event = json.load(sys.stdin)
    cwd = event.get("cwd") if isinstance(event, dict) else None
    root = Path(cwd) if isinstance(cwd, str) else Path.cwd()
    result = decision(event, root) if isinstance(event, dict) else None
    if result:
        print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
