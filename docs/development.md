# Develop the Codex connector

Run commands from the repository root. The `codex` crate builds `libai_gateway_connector_codex.so`; it has no server process.

## Prerequisites

Use the toolchain in `rust-toolchain.toml` with rustfmt and Clippy, and the SDK Git revision in the Cargo manifest and lockfile. Native packaging also needs a C toolchain, Python 3.11+, Git and `readelf` (binutils). Use a supported GNU Linux host; the [release gate](releasing.md) checks the library's actual glibc requirements, not just its build target.

## Source and tests

| File under `codex/src/` | Responsibility |
| --- | --- |
| `lib.rs`, `attempt.rs` | Native manifest, dispatch, operation descriptors, request plans |
| `protocol.rs` | OAuth, models, quota plans and response parsing |
| `settings.rs`, `identity.rs` | Settings, credential-scoped installation IDs and request context |
| `request_policy.rs`, `request-allowlists.json` | Provider field and header filtering; privacy substitutions |

Run the local checks without provider credentials:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python3 scripts/test-release-package.py
```

For a native development archive, run `./scripts/verify-release.sh VERSION --development` with the workspace version. It includes the checks above, builds the locked release target and selects the cdylib reported by Cargo. The archive is written under `development-package/` in the configured Cargo target directory with `source_revision: "uncommitted"`.

## Archive contents

Each `.tar.gz` contains the `.so`, exported `manifest.json`, `build-info.json`, per-file `SHA256SUMS`, documentation, project license and third-party notices and licenses. `build-info.json` identifies the target, ABI, connector version, glibc baseline, library digest, source and SDK. A companion `.sha256` covers the archive. Native verification loads the library and calls its operation descriptors; only run it on trusted artifacts.
