#!/usr/bin/env python3
import ast
import io
import re
import subprocess
import sys
import tokenize
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
C_LIKE = {".rs", ".swift", ".h", ".m", ".c"}
HASH = {".sh", ".py"}
ALLOWED_FIRST_LINES = ("#!", "// swift-tools-version:")
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


@dataclass(frozen=True)
class Comment:
    start: int
    end: int
    line: int


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def ident_before(text: str, i: int) -> bool:
    return i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_")


def skip_block(text: str, i: int) -> int:
    depth = 0
    while i < len(text):
        if text.startswith("/*", i):
            depth += 1
            i += 2
        elif text.startswith("*/", i):
            depth -= 1
            i += 2
            if depth == 0:
                return i
        else:
            i += 1
    return i


def skip_quoted(text: str, i: int, quote: str) -> int:
    i += 1
    while i < len(text) and text[i] != quote:
        i += 2 if text[i] == "\\" else 1
    return i + 1


def skip_rust_raw(text: str, i: int) -> int | None:
    m = re.compile(r'(?:br|cr|r)(#*)"').match(text, i)
    if not m:
        return None
    closing = '"' + m.group(1)
    end = text.find(closing, m.end())
    return len(text) if end < 0 else end + len(closing)


def skip_rust_quote(text: str, i: int) -> int:
    if text.startswith("'\\", i):
        return skip_quoted(text, i, "'")
    if i + 2 < len(text) and text[i + 2] == "'":
        return i + 3
    m = IDENT.match(text, i + 1)
    return m.end() if m else i + 1


def skip_swift_string(text: str, i: int) -> int:
    hashes = len(text[i:]) - len(text[i:].lstrip("#"))
    j = i + hashes
    triple = text.startswith('"""', j)
    delimiter = ('"""' if triple else '"') + "#" * hashes
    escape = "\\" + "#" * hashes
    j += 3 if triple else 1
    while j < len(text):
        if text.startswith(escape + "(", j):
            j = skip_swift_code(text, j + len(escape) + 1, closing=")")
        elif text.startswith(escape, j):
            j += len(escape) + 1
        elif text.startswith(delimiter, j):
            return j + len(delimiter)
        else:
            j += 1
    return j


def skip_swift_code(text: str, i: int, closing: str) -> int:
    depth = 0
    while i < len(text):
        c = text[i]
        if c == '"' or (c == "#" and re.compile(r'#+"').match(text, i)):
            i = skip_swift_string(text, i)
        elif text.startswith("/*", i):
            i = skip_block(text, i)
        elif text.startswith("//", i):
            i = text.find("\n", i) if "\n" in text[i:] else len(text)
        elif c == "(":
            depth += 1
            i += 1
        elif c == closing and depth == 0:
            return i + 1
        else:
            depth -= c == ")"
            i += 1
    return i


def c_like_comments(text: str, suffix: str) -> list[Comment]:
    out: list[Comment] = []
    i = 0
    while i < len(text):
        c = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = len(text) if end < 0 else end
            out.append(Comment(i, end, line_of(text, i)))
            i = end
        elif text.startswith("/*", i):
            end = skip_block(text, i)
            out.append(Comment(i, end, line_of(text, i)))
            i = end
        elif suffix == ".rs" and not ident_before(text, i) and (raw := skip_rust_raw(text, i)) is not None:
            i = raw
        elif suffix == ".rs" and c == "'":
            i = skip_rust_quote(text, i)
        elif suffix == ".swift" and (c == '"' or re.compile(r'#+"').match(text, i)):
            i = skip_swift_string(text, i)
        elif c == '"' or (c == "'" and suffix in {".h", ".m", ".c"}):
            i = skip_quoted(text, i, c)
        else:
            i += 1
    return out


HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def shell_comments(text: str) -> list[Comment]:
    out: list[Comment] = []
    offset = 0
    lines = text.splitlines(keepends=True)
    heredoc_end: str | None = None
    for line in lines:
        if heredoc_end is not None:
            if line.strip() == heredoc_end:
                heredoc_end = None
            offset += len(line)
            continue
        quote: str | None = None
        k = 0
        while k < len(line):
            c = line[k]
            if quote:
                if c == "\\" and quote == '"':
                    k += 1
                elif c == quote:
                    quote = None
            elif c in "'\"":
                quote = c
            elif c == "\\":
                k += 1
            elif c == "#" and (k == 0 or line[k - 1] in " \t;(|&") and not line.startswith("#!", k):
                end = len(line.rstrip("\n"))
                out.append(Comment(offset + k, offset + end, line_of(text, offset + k)))
                break
            k += 1
        if m := HEREDOC.search(line):
            heredoc_end = m.group(2)
        offset += len(line)
    return out


def python_comments(text: str) -> list[Comment]:
    starts = [0]
    for line in text.splitlines(keepends=True):
        starts.append(starts[-1] + len(line))
    out = [
        Comment(starts[t.start[0] - 1] + t.start[1], starts[t.end[0] - 1] + t.end[1], t.start[0])
        for t in tokenize.generate_tokens(io.StringIO(text).readline)
        if t.type == tokenize.COMMENT
    ]
    for node in ast.walk(ast.parse(text)):
        if isinstance(node, ast.Module | ast.ClassDef | ast.FunctionDef | ast.AsyncFunctionDef) and node.body:
            first = node.body[0]
            if (
                isinstance(first, ast.Expr)
                and isinstance(first.value, ast.Constant)
                and isinstance(first.value.value, str)
            ):
                start = starts[first.lineno - 1] + first.col_offset
                end = starts[(first.end_lineno or first.lineno) - 1] + (first.end_col_offset or 0)
                out.append(Comment(start, end, first.lineno))
    return sorted(out, key=lambda c: c.start)


def comments_in(path: Path, text: str) -> list[Comment]:
    if path.suffix in C_LIKE:
        found = c_like_comments(text, path.suffix)
    elif path.suffix == ".py":
        found = python_comments(text)
    else:
        found = shell_comments(text)
    first_line = text.split("\n", 1)[0]
    return [c for c in found if not (c.line == 1 and first_line.startswith(ALLOWED_FIRST_LINES) and c.start == 0)]


def strip(text: str, comments: list[Comment]) -> str:
    for c in sorted(comments, key=lambda c: c.start, reverse=True):
        line_start = text.rfind("\n", 0, c.start) + 1
        line_end = text.find("\n", c.end)
        line_end = len(text) if line_end < 0 else line_end
        before, after = text[line_start : c.start], text[c.end : line_end]
        if not before.strip() and not after.strip():
            text = text[:line_start] + text[min(line_end + 1, len(text)) :]
        else:
            text = text[:line_start] + before.rstrip() + after + text[line_end:]
    return text.lstrip("\n")


def tracked_sources() -> list[Path]:
    files = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return [ROOT / f for f in files.stdout.splitlines() if Path(f).suffix in C_LIKE | HASH and (ROOT / f).is_file()]


def main(argv: list[str]) -> int:
    fix = "--fix" in argv
    paths = [Path(a).resolve() for a in argv if not a.startswith("--")] or tracked_sources()
    failures = 0
    for path in paths:
        text = path.read_text()
        found = comments_in(path, text)
        if not found:
            continue
        if fix:
            path.write_text(strip(text, found))
            continue
        failures += len(found)
        for c in found:
            snippet = text[c.start : c.end].splitlines()[0][:80]
            print(f"{path.relative_to(ROOT)}:{c.line}: comment: {snippet}")
    if failures:
        print(f"\n{failures} comments found. Code carries no comments: see AGENTS.md, 'Code standards'.")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
