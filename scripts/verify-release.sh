#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# != 1 ]]; then
  echo "usage: verify-release.sh VERSION" >&2
  exit 2
fi
./scripts/check-release-version.sh "$1"
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/package-release.sh "$1"
