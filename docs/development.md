# Development

This repository is a Rust workspace with one provider crate, `codex`. The plugin
is a `cdylib`; it is not a standalone server. Development and package checks do
not need provider credentials or paid upstream requests.

## Prerequisites

- The Rust 1.97.1 toolchain with `rustfmt` and Clippy, as pinned in
  `rust-toolchain.toml`.
- Git access to the pinned connector SDK dependency.
- For native packaging: an x86_64 or aarch64 GNU Linux host, a C toolchain,
  Python 3.11 or newer, and `readelf` from binutils.

Official artifacts are built in the digest-pinned bookworm container specified
in CI. A newer local libc can produce a library rejected by the package gate;
building successfully is not sufficient to establish release compatibility.

Run the commands below from the repository root.

## Source layout

| Path | Responsibility |
| --- | --- |
| `codex/src/lib.rs` | Exported manifest and command dispatch |
| `codex/src/attempt.rs` | Operation descriptors and request adaptation plans |
| `codex/src/protocol.rs` | OAuth, model, and quota protocol planning/parsing |
| `codex/src/settings.rs` | Settings descriptor, defaults, validation, and compilation |
| `codex/src/identity.rs` | Credential-stable installation identity and request context |
| `codex/src/request_policy.rs` | Outbound filtering and privacy normalization |
| `codex/src/request-allowlists.json` | Embedded provider request policy |
| `scripts/` | Version checks, native packaging, and package-verifier tests |
| `.github/workflows/` | Two-architecture CI and tag publication |

Keep provider-specific behavior here. Transport execution, durable credentials,
routing, usage storage, billing, and logging belong in the gateway. See
[Codex compatibility](codex.md) before changing commands or metadata contracts.

## Run checks

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python3 scripts/test-release-package.py
bash -n scripts/*.sh
```

Rust tests are colocated with the implementation. Python tests cover archive
validation and package metadata. Do not use production credentials to run these
checks.

For documentation-only edits, check links, command definitions, and consistency
with the implementation; no upstream request or native build is needed solely
to validate prose.

## Build a release archive locally

With the current workspace version and dated changelog entry:

```sh
./scripts/package-release.sh 0.2.0
```

This builds the native release target, reads the real library's exported ABI
descriptor, packages it, and verifies the result. For the combined formatting,
Clippy, Rust tests, Python package tests, build, and archive gate, use:

```sh
./scripts/verify-release.sh 0.2.0
```

These commands are local checks, not publication. The example matches the
current workspace version; it does not make current protocol-3 source identical
to the historical 0.2.0 release. Choose a new version before publishing changed
code. Packaging honors `CARGO_TARGET_DIR`, including isolated build directories.

## Archive contents

The default output directory is `target/release-package/`, or
`$CARGO_TARGET_DIR/release-package/` when overridden. Each architecture produces
`ai-gateway-connector-codex-VERSION-TARGET.tar.gz` and a matching `.sha256` file.

Each archive has a single package directory containing:

- `libai_gateway_connector_codex.so` and its exported `manifest.json`.
- `build-info.json`: build-info schema, connector version and ABI, target, glibc
  baseline, library digest, source repository/revision, and SDK source.
- `SHA256SUMS` covering all other package files.
- `README.md`, `CHANGELOG.md`, and `docs/`.
- `LICENSE`, `THIRD_PARTY_NOTICES.md`, and dependency license texts in `LICENSES/`.

Verification checks the full checksum set, ELF architecture, glibc requirements,
native ABI/manifest, pure operation descriptors, and required license materials.
It loads the library to inspect it; only verify trusted native artifacts in this
way. For publication requirements, see [release maintenance](releasing.md).
