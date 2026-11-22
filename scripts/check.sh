#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

RUFF=ruff@0.17.6
MYPY=mypy@1.18.2
ZIZMOR=zizmor@1.33.0
MIN_COVERAGE=85
SWIFT_SOURCES=(mac/Package.swift mac/Sources mac/Tests bench)
SHELL_SCRIPTS=(scripts/*.sh scripts/hooks/* mac/scripts/*.sh xr/build.sh)
HEADER=mac/Sources/CLink/include/link.h

usage() {
  cat >&2 << 'USAGE'
usage: scripts/check.sh [all | fast | rust | android | swift | scripts | fix]
  all      every check; what CI runs, and the bar for calling a change done
  fast     formatting, lints and Rust tests; the pre-commit hook
  rust     format, clippy, tests, coverage, C header drift, dependency policy
  android  clippy for the headset target, then the APK build
  swift    format, SwiftLint, Swift 6 build with warnings as errors, tests, bench typecheck
  scripts  limits, no comments, spelling, Python, shell and workflow linters
  fix      apply every formatter and regenerate the C header
USAGE
  exit 2
}

run() {
  printf '\n==> %s\n' "$1"
  shift
  "$@"
}
fail() {
  echo "check: $*" >&2
  exit 1
}
need() {
  local tool
  for tool in "$@"; do
    command -v "$tool" > /dev/null || fail "missing $tool. Install it: docs/development.md#install-the-toolchains"
  done
}
uvtool() { uvx -q "$@"; }
header() {
  cbindgen --quiet --config crates/link/cbindgen.toml crates/link/src/ffi.rs
}

check_scripts() {
  need uvx typos shfmt shellcheck actionlint
  run "file limits and whitespace" python3 scripts/limits.py
  run "code has no comments" python3 scripts/comments.py
  run "spelling" typos
  run "script tests" python3 -m unittest discover -s scripts -p 'test_*.py' -q
  run "python format" uvtool "$RUFF" format --check --quiet .
  run "python lint" uvtool "$RUFF" check --quiet .
  run "python types" uvtool "$MYPY"
  run "shell format" shfmt -d "${SHELL_SCRIPTS[@]}"
  run "shellcheck" shellcheck "${SHELL_SCRIPTS[@]}"
  run "workflows" actionlint
  run "workflow security" uvtool "$ZIZMOR" --quiet .github/workflows
}

check_rust_lint() {
  run "rust format" cargo fmt --all --check
  run "rust clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
}

header_is_current() {
  local generated
  generated="$(header)"
  diff -u "$HEADER" <(printf '%s\n' "$generated") || fail "link.h is stale; run scripts/check.sh fix"
}

check_rust() {
  need cargo-llvm-cov cargo-deny cargo-shear cbindgen
  check_rust_lint
  run "rust tests, at least ${MIN_COVERAGE}% line coverage" \
    cargo llvm-cov --workspace --exclude xr --locked --fail-under-lines "$MIN_COVERAGE" --summary-only
  run "headset logic tests" cargo test -p xr --locked
  run "C header matches crates/link/src/ffi.rs" header_is_current
  run "dependency policy" cargo deny --log-level error check
  run "unused dependencies" cargo shear
}

check_android() {
  source scripts/android-env.sh
  run "headset clippy" cargo clippy -p xr --target aarch64-linux-android --all-targets --locked -- -D warnings
  run "headset apk" xr/build.sh
}

typecheck_bench() {
  local file
  for file in bench/*.swift; do
    if grep -q CGVirtualDisplay "$file"; then
      swiftc -typecheck -swift-version 5 -import-objc-header bench/CGVirtualDisplay.h "$file"
    else
      swiftc -typecheck -swift-version 5 "$file"
    fi
  done
}

check_swift_style() {
  need swift swiftlint
  run "swift format" swift format lint --strict --recursive --parallel "${SWIFT_SOURCES[@]}"
  run "swiftlint" swiftlint lint --quiet --strict
}

check_swift() {
  check_swift_style
  run "link static library" cargo build --release -p link --locked
  run "swift 6 build, warnings are errors" swift build --package-path mac --build-tests -Xswiftc -warnings-as-errors
  run "swift tests" swift test --package-path mac --skip-build
  run "bench scripts typecheck" typecheck_bench
}

fix() {
  cargo fmt --all
  swift format --in-place --recursive --parallel "${SWIFT_SOURCES[@]}"
  uvtool "$RUFF" format --quiet .
  uvtool "$RUFF" check --fix --exit-zero --quiet .
  shfmt -w "${SHELL_SCRIPTS[@]}"
  header > "$HEADER"
}

target="${1:-all}"
case "$target" in
  all)
    check_scripts
    check_rust
    check_swift
    check_android
    ;;
  fast)
    check_scripts
    check_rust_lint
    check_swift_style
    run "rust tests" cargo test --workspace --locked
    ;;
  rust | android | swift | scripts) "check_$target" ;;
  fix) fix ;;
  *) usage ;;
esac
printf '\ncheck %s: passed\n' "$target"
