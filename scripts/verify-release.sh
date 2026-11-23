#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# != 1 && !( $# == 2 && $2 == --development ) ]]; then
  echo "usage: verify-release.sh VERSION [--development]" >&2
  exit 2
fi
python3 scripts/test-release-package.py
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/package-release.sh "$@"
