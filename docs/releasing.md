# Release maintenance

Gateway and connector releases are independent. This workflow publishes
connector archives; it never installs them into a gateway or changes an
administrator's active version. Do not bundle Codex into gateway binaries or
images or add silent installation or fallback.

## Prepare a version

Update `[workspace.package].version` in `Cargo.toml`, add a dated entry in
`CHANGELOG.md`, and update version examples together. Update `Cargo.lock` for the
workspace crate's new version too, so the locked build gates accept the release.
Keep historical changelog entries intact. Never move or reuse a published tag.

Version 0.2.1 declares protocol 3 while the 0.2.0 release history records
protocol 2. Select a new package version before publishing a changed contract.

Pin `ai-gateway-connector-sdk` to a reviewed 40-character Git commit. Regenerate
`Cargo.lock` when the dependency changes as well. Include the updated lockfile and
manifest in the release commit for either kind of change. Local path dependencies
are only for joint SDK development; tag preflight rejects them.

## Check the contract

ABI 1 exports `ai_gateway_connector_entry_v1`. The manifest currently declares
command protocol 3 and settings schema 1. The reviewed SDK pin is retained
because its ABI-1 JSON encoding supports the metadata additions; do not import
unpublished SDK source solely to change protocol metadata.

The package gate requires protocol 3 and the commands the gateway uses for
settings, request preparation (including both image edit plans), OAuth, models,
and quota management. It checks `attempt.capabilities` and `attempt.describe/v1`,
then dispatches the pure, bounded descriptor for each operation without
credentials or I/O. Descriptors must select operation-specific transport,
response pass-through, and the gateway's `general` parser for upstream Responses
usage. Response-adapter commands are rejected.

Native ABI and command protocol are separate contracts. Publish only a connector
and host pairing that supports both. See [Codex compatibility](codex.md).

## Run the local gate

Use the pinned toolchain and a native GNU Linux environment compatible with the
official glibc baseline. Prerequisites are listed in [development](development.md#prerequisites).

With the current workspace version:

```sh
./scripts/check-release-version.sh 0.2.1 --require-pinned-sdk
./scripts/verify-release.sh 0.2.1
bash -n scripts/*.sh
```

Replace `0.2.1` after preparing a new version. Release packaging requires a clean
Git checkout, a pinned SDK, and the matching version committed in `Cargo.toml`.
Before committing, use `verify-release.sh VERSION --development` to exercise
the package gate without claiming a source commit or creating release assets.

The combined gate runs package-verifier tests, formatting, locked Clippy and
workspace tests, then builds and verifies the actual shared-library archive.
No provider secrets or real API requests are used. Packaging honors
`CARGO_TARGET_DIR`.

Verification checks the complete checksum set, native ABI and exported manifest,
library digest, required license materials, ELF architecture, and GNU libc symbol
requirements no newer than 2.36. Missing dependency license texts or a newer libc
requirement fail the gate. To verify a trusted archive independently, run
`python3 scripts/release-package.py verify ARCHIVE` in this repository with the
recorded source commit available locally. The verifier checks the repository
identity, connector version in that commit's `Cargo.toml`, and exact SDK Git
source in its `Cargo.lock`. These are consistency checks, not a signature or a
reproducible-build attestation. It rejects development archives unless you explicitly pass `--development`.
See [archive contents](development.md#archive-contents).

## Publish an annotated tag

After the reviewed release commit is on `main` and CI is green, create and push
an annotated `vX.Y.Z` tag for the prepared version. For example, only when 0.3.0
has actually been prepared:

```sh
git tag -a v0.3.0 -m "Release v0.3.0"
git push origin v0.3.0
```

The tag workflow verifies that the tag is annotated, resolves to the triggering
commit on `main`, matches the workspace version and dated changelog entry, and
uses an immutable SDK pin.

Read-only native jobs build both targets:

| Runner | Target |
| --- | --- |
| `ubuntu-24.04` | `x86_64-unknown-linux-gnu` |
| `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` |

Both run inside the same digest-pinned `rust:1.97.1-bookworm` image as the gateway,
not against the runners' newer libc. The gate checks ELF requirements before
loading the ABI descriptor and records the 2.36 baseline in `build-info.json`.

Only the final publisher receives `contents: write`; it does not build untrusted
code. It publishes both verified archives and their `.sha256` files after all
builds pass. Actions are pinned to full commit SHAs. CI is read-only and does not
write caches, including on pull requests.

## Confirm the release

Check that the GitHub Release contains both architecture archives and both
checksum files. GitHub supplies source archives for the exact tag, and package
metadata records the source commit and SDK source. Preserve corresponding-source
availability and the AGPL and dependency license materials when redistributing.

Activation is a separate administrator action. Link users to
[installation and upgrades](deployment.md), including the matching gateway
requirements, rather than implying publication enables the connector.
