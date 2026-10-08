#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - "$@" <<'PY'
import datetime
import pathlib
import re
import sys
import tomllib

args = sys.argv[1:]
if not args or len(args) > 2 or (len(args) == 2 and args[1] != "--require-pinned-sdk"):
    raise SystemExit("usage: check-release-version.sh VERSION [--require-pinned-sdk]")
version = args[0].removeprefix("v")
if not re.fullmatch(r"\d+\.\d+\.\d+", version):
    raise SystemExit("release version must be X.Y.Z")
manifest = tomllib.loads(pathlib.Path("Cargo.toml").read_text())
if manifest["workspace"]["package"]["version"] != version:
    raise SystemExit("Cargo version does not match release")
changelog = pathlib.Path("CHANGELOG.md").read_text()
entry = re.search(rf"^## \[{re.escape(version)}\] - (\d{{4}}-\d{{2}}-\d{{2}})$", changelog, re.M)
if entry is None:
    raise SystemExit("missing dated changelog entry")
datetime.date.fromisoformat(entry[1])
if len(args) == 2:
    sdk = manifest["workspace"]["dependencies"]["ai-gateway-connector-sdk"]
    if "path" in sdk or not sdk.get("git") or not re.fullmatch(r"[a-f0-9]{40}", sdk.get("rev", "")):
        raise SystemExit("release SDK must use a full immutable Git revision, not a local path")
print(f"release version verified: {version}")
PY
