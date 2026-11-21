#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
git config core.hooksPath scripts/hooks
echo "git hooks enabled: pre-commit runs scripts/check.sh fast, commit-msg checks Conventional Commits"
