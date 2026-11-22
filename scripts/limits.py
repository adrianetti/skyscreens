#!/usr/bin/env python3
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CODE_SUFFIXES = {".rs", ".swift", ".py", ".sh", ".h", ".m", ".c"}
TEXT_SUFFIXES = CODE_SUFFIXES | {".md", ".toml", ".yml", ".yaml", ".json", ".txt", ".xml", ".svg"}
MAX_LINES = 400
MAX_LINES_RESEARCH = 500
MAX_LINES_SHELL = 150
RESEARCH_DIR = "bench/"
SHELL_HEADER = ("#!/usr/bin/env bash", "set -euo pipefail")
EXECUTABLE_MODE = "100755"


@dataclass(frozen=True)
class TrackedFile:
    path: str
    mode: str


def is_shell(path: str) -> bool:
    return path.endswith(".sh") or path.startswith("scripts/hooks/")


def is_code(path: str) -> bool:
    return Path(path).suffix in CODE_SUFFIXES or is_shell(path)


def line_limit(path: str) -> int:
    if is_shell(path):
        return MAX_LINES_SHELL
    if path.startswith(RESEARCH_DIR):
        return MAX_LINES_RESEARCH
    return MAX_LINES


def whitespace_problems(path: str, text: str) -> list[str]:
    problems = []
    if text and not text.endswith("\n"):
        problems.append("does not end with a newline")
    if text.endswith("\n\n"):
        problems.append("ends with blank lines")
    if "\r" in text:
        problems.append("has CRLF line endings")
    for number, line in enumerate(text.split("\n"), start=1):
        content = line.removesuffix("\r")
        if content != content.rstrip() and not path.endswith(".md"):
            problems.append(f"line {number} has trailing whitespace")
        if "\t" in line and is_code(path):
            problems.append(f"line {number} has a tab")
    return problems


def code_problems(file: TrackedFile, text: str) -> list[str]:
    problems = []
    lines = text.count("\n")
    limit = line_limit(file.path)
    if lines > limit:
        problems.append(f"{lines} lines, the limit is {limit}; split it")
    if is_shell(file.path) and file.mode == EXECUTABLE_MODE and tuple(text.split("\n")[:2]) != SHELL_HEADER:
        problems.append("an executable script starts with '#!/usr/bin/env bash' then 'set -euo pipefail'")
    return problems


def problems_in(file: TrackedFile, text: str) -> list[str]:
    found = whitespace_problems(file.path, text)
    if is_code(file.path):
        found += code_problems(file, text)
    return found


def tracked_files() -> list[TrackedFile]:
    listing = subprocess.run(["git", "ls-files", "-s"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    files = [TrackedFile(path=line.split("\t", 1)[1], mode=line.split()[0]) for line in listing.splitlines()]
    untracked = subprocess.run(
        ["git", "ls-files", "-o", "--exclude-standard"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    files += [
        TrackedFile(path=path, mode=EXECUTABLE_MODE if executable(path) else "100644") for path in untracked.split()
    ]
    return [
        f for f in files if (Path(f.path).suffix in TEXT_SUFFIXES or is_shell(f.path)) and (ROOT / f.path).is_file()
    ]


def executable(path: str) -> bool:
    return (ROOT / path).stat().st_mode & 0o111 != 0


def main() -> int:
    failures = 0
    for file in tracked_files():
        for problem in problems_in(file, (ROOT / file.path).read_text()):
            failures += 1
            print(f"{file.path}: {problem}")
    if failures:
        print(f"\n{failures} problems. Limits are in docs/code-standards.md.")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
