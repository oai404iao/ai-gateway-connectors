# Repository guidance

This is the native connector workspace for ai-gateway, currently containing the
`codex` provider. It is not the gateway application. Keep its independent build,
release, and installation boundary intact.

## Where changes belong

- `codex/src/lib.rs` exports the manifest and dispatches commands.
- `attempt.rs` owns operation descriptors and request plans; `protocol.rs` owns
  OAuth, model, and quota planning/parsing.
- `settings.rs` owns the five-field settings schema; `identity.rs` owns request
  context and credential-stable installation IDs.
- `request_policy.rs` and `request-allowlists.json` own provider outbound filtering
  and privacy normalization.
- `scripts/` and `.github/workflows/` own native packaging and release gates.

Gateway transport, credential persistence, routing, billing, and request logging
belong in the gateway, not in a new plugin runtime.

## Commands

Run from the repository root with the toolchain in `rust-toolchain.toml`:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python3 scripts/test-release-package.py
bash -n scripts/*.sh
```

For packaging changes, use `./scripts/verify-release.sh 0.2.0` with the current
workspace version on a supported native GNU Linux host. It builds and loads the
real library and verifies its archive. See [development](docs/development.md) for
prerequisites and output paths. Local checks need no provider credentials or
paid upstream calls.

For documentation-only changes, check paths, links, and claims against the source;
do not run a native build or call a provider solely to validate prose.

## Contracts to preserve

- Treat ABI 1, manifest protocol 3, and settings schema 1 as separate contracts.
  Read [Codex compatibility](docs/codex.md) before changing host-facing metadata.
- Keep operation responses pass-through. The gateway's `general` parser consumes
  upstream Responses usage; the plugin does not add response adapters or billing.
- Consume compiled settings through `metadata.settings`, not legacy host-owned
  identity fields. Preserve credential-stable installation IDs and privacy
  filtering; do not restore metadata removed by gateway financial policy.
- Keep the SDK pinned to a reviewed full Git revision for releases. Never bundle
  the connector into gateway binaries/images or add silent installation/fallback.

## Documentation and releases

Documentation is English, with concise task-oriented headings, relative links
between pages, tables for reference values, and `sh` command examples. Start at
[docs/index.md](docs/index.md). Update affected links when moving or splitting a
page. `LICENSE` is legal text, not prose to rewrite.

Read [release maintenance](docs/releasing.md) for version, changelog, SDK, and tag
changes. The current source's protocol 3 is unreleased relative to the 0.2.0
history; do not reuse that tag. Documentation edits do not authorize publishing,
deployment, or changing an installed gateway.
