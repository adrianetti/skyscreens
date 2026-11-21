#!/usr/bin/env python3
import re
import subprocess
import sys

TYPES = ("feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert")
SUBJECT = re.compile(rf"^(?:{'|'.join(TYPES)})(?:\([a-z0-9][a-z0-9-]*\))?!?: \S.*$")
MAX_LENGTH = 100


def problems(subject: str) -> list[str]:
    found = []
    if not SUBJECT.match(subject):
        found.append(f"not Conventional Commits: '<type>(<scope>): <summary>' with type in {', '.join(TYPES)}")
    if len(subject) > MAX_LENGTH:
        found.append(f"{len(subject)} characters, the limit is {MAX_LENGTH}")
    if subject.endswith("."):
        found.append("ends with a period")
    return found


def subjects_in_range(revision_range: str) -> list[str]:
    log = subprocess.run(
        ["git", "log", "--no-merges", "--format=%s", revision_range], capture_output=True, text=True, check=True
    )
    return [line for line in log.stdout.splitlines() if line]


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[0] == "--file":
        with open(argv[1]) as f:
            lines = [line for line in f.read().splitlines() if line and not line.startswith("#")]
        subjects = lines[:1]
    elif len(argv) == 2 and argv[0] == "--range":
        subjects = subjects_in_range(argv[1])
    else:
        print("usage: commit_msg.py --file <message file> | --range <base>..<head>", file=sys.stderr)
        return 2
    bad = [(s, p) for s in subjects for p in problems(s)]
    for subject, problem in bad:
        print(f"commit message '{subject}': {problem}", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
