# Release maintenance

> Status: Current independent connector release workflow.

Gateway and connector releases are independent. Never add this library to a
gateway binary/image release or implement silent installation/fallback.

## Version and compatibility

Update `[workspace.package].version`, the dated `CHANGELOG.md` section, and
examples together. Pin `ai-gateway-connector-sdk` to a reviewed **40-character
Git commit**, regenerate `Cargo.lock`, and include both in the release commit.
A local path dependency is permitted only while jointly developing the SDK;
the tag workflow rejects it.

ABI v1 has one C entry point, `ai_gateway_connector_entry_v1`. This connector
declares manifest `protocol_version: 3` for explicit transport and upstream-usage
descriptors, preserving plugin-owned identity, privacy and settings.
The package gate requires that marker, `attempt.describe/v1`, legacy
`attempt.capabilities`, `attempt.context` and the three settings
describe/validate/compile commands. It validates the bounded, typed descriptors
for all operations through pure native dispatch without credentials or I/O.
Descriptors must select response pass-through and the gateway's general parser
for the actual upstream Responses usage interface; response-adapter commands
are rejected. Incompatible gateways reject the marker
rather than executing the changed metadata contract incorrectly. C ABI version
and command protocol version are independent; release compatible pairs.

The existing reviewed SDK Git pin is retained: its ABI-1 JSON encoding supports
these metadata additions without importing unpublished SDK source. Settings
schema remains 1. Native protocol metadata is not a package-version release;
do not move or reuse an existing tag. Packaging honors `CARGO_TARGET_DIR`, including
isolated development targets outside the source tree.

## Local gate

Use the pinned Rust toolchain and a native GNU Linux host:

```sh
./scripts/check-release-version.sh 0.2.0 --require-pinned-sdk
./scripts/verify-release.sh 0.2.0
```

This runs formatting, locked workspace Clippy/tests, builds and verifies the
actual shared library archive. No provider secrets or real API calls are used.
Packaging fails if required dependency license texts cannot be found. Archive
verification checks its complete checksum set, descriptor ABI/manifest,
recorded library digest, required license materials, ELF architecture, and GNU
libc symbol requirements no newer than 2.36. Packaging requires `readelf`
(binutils) and rejects artifacts built against a newer libc.

## Tag pipeline

After the reviewed release commit is on `main` and CI is green:

```sh
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

Never move/reuse a published tag. The workflow verifies an annotated tag pointing
to `main`, version/dated changelog consistency, and an immutable SDK pin. Read-only
native build jobs run locked quality gates and package both:

- `ubuntu-24.04`: `x86_64-unknown-linux-gnu`
- `ubuntu-24.04-arm`: `aarch64-unknown-linux-gnu`

Both native runners build inside the same digest-pinned
`rust:1.97.1-bookworm` image as the gateway, not against the runner's newer libc.
The package gate checks the ELF version requirements before loading its ABI
descriptor and records the 2.36 baseline in `build-info.json`.

Only the final publisher gets `contents: write`; it does not build untrusted code.
It publishes the verified archives and per-archive checksums after **all** builds
pass. GitHub automatically supplies source archives for the exact tag; package
metadata records the source commit. Preserve corresponding source availability.

Actions are pinned to full commit SHAs. CI has read-only permissions and no cache
writes (including pull requests). Release jobs never install artifacts into the
gateway or change the administrator's selected active version. Confirm both architecture artifacts and
their `.sha256` files on the final GitHub Release.
