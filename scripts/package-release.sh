#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# != 1 ]]; then
  echo "usage: package-release.sh VERSION" >&2
  exit 2
fi
version="${1#v}"
./scripts/check-release-version.sh "$version"
target="$(rustc -vV | awk '$1 == "host:" { print $2 }')"
case "$target" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;;
  *) echo "unsupported native release target: $target" >&2; exit 1 ;;
esac
cargo build --locked --release --package ai-gateway-connector-codex --target "$target"
python3 scripts/release-package.py build "$version" "$target"
