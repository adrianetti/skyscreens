#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

RUFF_VERSION=0.17.6
SWIFT_SOURCES=(mac/Package.swift mac/Sources mac/Tests bench)
HEADER=mac/Sources/CLink/include/link.h

usage() {
  cat >&2 << 'USAGE'
usage: scripts/check.sh [all | fast | rust | android | swift | scripts | fix]
  all      every check; what CI runs, and the bar for calling a change done
  fast     formatting, lints and Rust tests; the pre-commit hook
  rust     format, clippy, tests, C header drift, dependency policy
  android  clippy for the headset target, then the APK build
  swift    format, warnings-as-errors build, tests, bench typecheck
  scripts  no-comment rule, Python, shell and workflow linters
  fix      apply every formatter and regenerate the C header
USAGE
  exit 2
}

step() { printf '\n==> %s\n' "$*"; }
fail() {
  echo "check: $*" >&2
  exit 1
}
need() { command -v "$1" > /dev/null || fail "missing $1. $2"; }

ruff() {
  if command -v uvx > /dev/null; then
    uvx -q "ruff@$RUFF_VERSION" "$@"
  else
    need ruff "Install uv (brew install uv) or ruff $RUFF_VERSION."
    command ruff "$@"
  fi
}

header() {
  need cbindgen "brew install cbindgen"
  cbindgen --quiet --config crates/link/cbindgen.toml crates/link/src/ffi.rs
}

check_scripts() {
  step "code has no comments"
  python3 scripts/comments.py
  step "script tests"
  python3 -m unittest discover -s scripts -p 'test_*.py' -q
  step "python format"
  ruff format --check --quiet .
  step "python lint"
  ruff check --quiet .
  step "shellcheck"
  need shellcheck "brew install shellcheck"
  git ls-files -co --exclude-standard '*.sh' 'scripts/hooks/*' | xargs shellcheck
  step "workflows"
  need actionlint "brew install actionlint"
  actionlint
}

check_rust_format_and_lint() {
  step "rust format"
  cargo fmt --all --check
  step "rust clippy"
  cargo clippy --workspace --all-targets --locked -- -D warnings
}

check_rust() {
  check_rust_format_and_lint
  step "rust tests"
  cargo test --workspace --locked
  step "C header matches crates/link/src/ffi.rs"
  local generated
  generated="$(header)"
  diff -u "$HEADER" <(printf '%s\n' "$generated") || fail "link.h is stale; run scripts/check.sh fix"
  step "dependency policy"
  need cargo-deny "brew install cargo-deny"
  cargo deny --log-level error check
}

check_android() {
  source scripts/android-env.sh
  step "headset clippy"
  cargo clippy -p xr --target aarch64-linux-android --all-targets --locked -- -D warnings
  step "headset apk"
  xr/build.sh
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

check_swift_format() {
  step "swift format"
  swift format lint --strict --recursive --parallel "${SWIFT_SOURCES[@]}"
}

check_swift() {
  need swift "Install Xcode."
  check_swift_format
  step "link static library"
  cargo build --release -p link --locked
  step "swift build, warnings are errors"
  (cd mac && swift build --build-tests -Xswiftc -warnings-as-errors)
  step "swift tests"
  (cd mac && swift test --skip-build)
  step "bench scripts typecheck"
  typecheck_bench
}

fix() {
  cargo fmt --all
  swift format --in-place --recursive --parallel "${SWIFT_SOURCES[@]}"
  ruff format --quiet .
  ruff check --fix --exit-zero --quiet .
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
    check_rust_format_and_lint
    check_swift_format
    step "rust tests"
    cargo test --workspace --locked
    ;;
  rust) check_rust ;;
  android) check_android ;;
  swift) check_swift ;;
  scripts) check_scripts ;;
  fix) fix ;;
  *) usage ;;
esac
printf '\ncheck %s: passed\n' "$target"
