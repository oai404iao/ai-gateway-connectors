# Release a connector package

Connector packages ship independently from the gateway. Publication makes an artifact available for installation; it does not activate it.

## Prepare and check the release

1. Set a new workspace version in `Cargo.toml`, update `Cargo.lock` and add a dated [changelog](../CHANGELOG.md) entry. Keep the SDK pinned to a reviewed full Git commit. Use a new version for a changed command contract.
2. Run `./scripts/verify-release.sh VERSION --development` to check uncommitted changes. On the clean release commit, run `./scripts/verify-release.sh VERSION` without that flag. This runs formatting, Clippy, Rust and package tests, then builds and verifies a native archive in the Cargo target directory's `release-package/`.
3. Check the [host command contract](codex.md#compatibility) against the gateway being paired with the artifact. The packager checks required request, settings, image, OAuth, discovery and quota commands. It calls `attempt.describe/v1` for each operation to verify transport, pass-through response mode and usage parser.

For an independent check of a trusted archive, run `python3 scripts/release-package.py verify ARCHIVE` from a connector checkout containing the recorded commit. The verifier checks checksums, native ABI, manifest, ELF target, glibc requirements and license materials. It compares the connector version and exact SDK Git source with `Cargo.toml` and `Cargo.lock` at the recorded revision. These checks do not sign the archive or establish reproducibility.

## Publish an annotated tag

After the release commit is reviewed on `main` and CI passes, create and push a new annotated `vX.Y.Z` tag for that commit. Tag preflight checks the commit ancestry, version, dated changelog entry and pinned SDK. Never move a published tag.

The workflow runs native builds for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` in the pinned Rust bookworm image. Only the final publishing job gets write access; it checks and publishes the two archives and their companion `.sha256` files. Verify all four assets are present and preserve corresponding source, licenses and notices when redistributing them.
