#!/usr/bin/env python3
import hashlib
import io
import json
import re
import subprocess
import sys
import tokenize
from collections import Counter
from pathlib import Path
from typing import TypedDict

from comments import skip_quoted, skip_rust_quote, skip_rust_raw, skip_swift_string

ROOT = Path(__file__).resolve().parent.parent


class Inventory(TypedDict, total=False):
    hatches: dict[str, dict[str, int]]
    fingerprints: dict[str, str]


LEDGER = ROOT / "escape-hatches.json"
POLICY = (
    "Every way to step around a rule, counted per file. scripts/check.sh fails when the code differs from this file. "
    "Only a human updates it, with scripts/escape_hatches.py --update, and a code owner approves the change."
)

PATTERNS: dict[str, list[tuple[str, re.Pattern[str]]]] = {
    ".rs": [
        ("rust unsafe", re.compile(r"\bunsafe\b")),
        ("rust transmute", re.compile(r"\btransmute\b")),
        ("rust discarded result", re.compile(r"\blet _\w* =")),
    ],
    ".swift": [
        ("swift unsafe", re.compile(r"(?<!\()\bunsafe\b(?!\))")),
        ("swift nonisolated(unsafe)", re.compile(r"nonisolated\(unsafe\)")),
        ("swift @unchecked Sendable", re.compile(r"@unchecked\s+Sendable")),
        ("swift MainActor.assumeIsolated", re.compile(r"MainActor\.assumeIsolated")),
        ("swift try?", re.compile(r"\btry\?")),
        ("swift discarded result", re.compile(r"(?<![\w.])_ = ")),
    ],
    ".py": [
        ("python Any", re.compile(r"\bAny\b")),
        ("python cast", re.compile(r"\bcast\(")),
    ],
    ".sh": [
        ("shell ignored failure", re.compile(r"\|\|\s*true\b")),
        ("shell silenced stderr", re.compile(r"2>\s*/dev/null")),
    ],
}
RUST_EXPECT = re.compile(r"#!?\[expect\(([^\]]*)\)\]", re.DOTALL)
SHA_PIN = re.compile(r"(uses:\s*[\w.-]+/[\w./-]+@)[0-9a-f]{40}")
VERSION = re.compile(r'"\d+(\.\d+)*"')
RULE_FILES = [
    "Cargo.toml",
    "crates/link/Cargo.toml",
    "crates/link-helper/Cargo.toml",
    "crates/fake-headset/Cargo.toml",
    "xr/Cargo.toml",
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
    ".github/CODEOWNERS",
    ".github/zizmor.yml",
    ".github/actionlint.yaml",
    ".github/dependabot.yml",
    ".github/workflows/ci.yml",
    ".github/workflows/claude.yml",
    ".github/workflows/claude-review.yml",
    ".claude/settings.json",
    "scripts/check.sh",
    "scripts/eval.sh",
    "scripts/eval_judge.py",
    "scripts/limits.py",
    "scripts/comments.py",
    "scripts/commit_msg.py",
    "scripts/escape_hatches.py",
    "scripts/agent_guard.py",
    "scripts/hooks/pre-commit",
    "scripts/hooks/commit-msg",
]


def blank(text: str, start: int, end: int) -> str:
    return text[:start] + "".join(c if c == "\n" else " " for c in text[start:end]) + text[end:]


def mask_c_like(text: str, suffix: str) -> str:
    i = 0
    while i < len(text):
        c = text[i]
        end = None
        if suffix == ".rs" and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            end = skip_rust_raw(text, i)
        if end is None and suffix == ".rs" and c == "'":
            end = skip_rust_quote(text, i)
            i = end
            continue
        if end is None and suffix == ".swift" and (c == '"' or re.compile(r'#+"').match(text, i)):
            end = skip_swift_string(text, i)
        if end is None and c == '"':
            end = skip_quoted(text, i, '"')
        if end is None:
            i += 1
            continue
        text = blank(text, i, end)
        i = end
    return text


def mask_python(text: str) -> str:
    lines = text.splitlines(keepends=True)
    starts = [0]
    for line in lines:
        starts.append(starts[-1] + len(line))
    for token in reversed(list(tokenize.generate_tokens(io.StringIO(text).readline))):
        if token.type == tokenize.STRING:
            start = starts[token.start[0] - 1] + token.start[1]
            end = starts[token.end[0] - 1] + token.end[1]
            text = blank(text, start, end)
    return text


def masked(path: str, text: str) -> str:
    suffix = Path(path).suffix
    if suffix in {".rs", ".swift"}:
        return mask_c_like(text, suffix)
    if suffix == ".py":
        return mask_python(text)
    return text


def language(path: str) -> str:
    return ".sh" if path.startswith("scripts/hooks/") else Path(path).suffix


def rust_expectations(text: str) -> list[str]:
    lints = []
    for match in RUST_EXPECT.finditer(text):
        for argument in match.group(1).split(","):
            name = argument.strip()
            if name and not name.startswith("reason"):
                lints.append(f"rust expect {name}")
    return lints


def hatches_in(path: str, text: str) -> Counter[str]:
    code = masked(path, text)
    found: Counter[str] = Counter()
    for kind, pattern in PATTERNS.get(language(path), []):
        count = len(pattern.findall(code))
        if count:
            found[kind] = count
    if path.endswith(".rs"):
        found.update(rust_expectations(code))
    return found


def fingerprint(path: str, text: str) -> str:
    if path.startswith(".github/workflows/"):
        text = SHA_PIN.sub(r"\1<pinned>", text)
    if path.endswith("Cargo.toml"):
        text = VERSION.sub('"<version>"', text)
    return "sha256:" + hashlib.sha256(text.encode()).hexdigest()


def source_files() -> list[str]:
    listing = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    return sorted(p for p in listing if language(p) in PATTERNS and (ROOT / p).is_file())


def inventory() -> Inventory:
    hatches: dict[str, dict[str, int]] = {}
    for path in source_files():
        for kind, count in sorted(hatches_in(path, (ROOT / path).read_text()).items()):
            hatches.setdefault(kind, {})[path] = count
    fingerprints = {
        path: fingerprint(path, (ROOT / path).read_text()) for path in RULE_FILES if (ROOT / path).is_file()
    }
    return {
        "hatches": {kind: dict(sorted(files.items())) for kind, files in sorted(hatches.items())},
        "fingerprints": fingerprints,
    }


def differences(expected: Inventory, actual: Inventory) -> list[str]:
    found = []
    old_hatches, new_hatches = expected.get("hatches", {}), actual.get("hatches", {})
    for kind in sorted(set(old_hatches) | set(new_hatches)):
        before, after = old_hatches.get(kind, {}), new_hatches.get(kind, {})
        found += [
            f"{kind}: {path} {before.get(path, 0)} -> {after.get(path, 0)}"
            for path in sorted(set(before) | set(after))
            if before.get(path) != after.get(path)
        ]
    old_prints, new_prints = expected.get("fingerprints", {}), actual.get("fingerprints", {})
    found += [
        f"rule file changed: {path}"
        for path in sorted(set(old_prints) | set(new_prints))
        if old_prints.get(path) != new_prints.get(path)
    ]
    return found


def main(argv: list[str]) -> int:
    actual = inventory()
    if argv == ["--update"]:
        LEDGER.write_text(json.dumps({"policy": POLICY, **actual}, indent=2) + "\n")
        print(f"updated {LEDGER.name}")
        return 0
    expected: Inventory = json.loads(LEDGER.read_text()) if LEDGER.exists() else Inventory()
    changed = differences(expected, actual)
    for line in changed:
        print(line)
    if changed:
        print(
            "\nThe escape hatches or rule files changed. Fix the code so it needs no new hatch. "
            "If the change is intended, ask a human to run scripts/escape_hatches.py --update; "
            "a code owner approves escape-hatches.json."
        )
    return 1 if changed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
