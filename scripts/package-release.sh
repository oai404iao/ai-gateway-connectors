#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# != 1 && !( $# == 2 && $2 == --development ) ]]; then
  echo "usage: package-release.sh VERSION [--development]" >&2
  exit 2
fi
version="${1#v}"
target="$(rustc -vV | awk '$1 == "host:" { print $2 }')"
exec python3 scripts/release-package.py build "$version" "$target" "${@:2}"
