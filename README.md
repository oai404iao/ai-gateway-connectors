# ai-gateway-connectors

> Status: Current external connector implementation.

Native provider connectors for [ai-gateway](https://github.com/oai404iao/ai_gateway).
The gateway retains its built-in `general` connector. **Codex is independently
built, released, and installed from this repository; it is not bundled in
gateway binaries or images.**

The Codex module implements OAuth protocol planning/parsing, model and quota
discovery, and provider adaptation for Responses HTTP/WebSocket, standalone web
search, and Images generation/edit. Gateway-managed HTTP transport, durable
credentials, routing, billing, and request logging remain gateway responsibilities.

## Compatibility

| Contract | Value |
| --- | --- |
| Connector ID | `codex` |
| Native SDK ABI | `1` |
| Release build-info schema | `1` |
| Command schema | SDK 0.1 / connector 0.1 |
| Library | `libai_gateway_connector_codex.so` |
| Release targets | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Build baseline | Pinned Debian bookworm Rust 1.97.1 container, GNU libc 2.36 or older requirements |

Use a gateway version built against the same SDK ABI and Codex command schema.
An ABI match alone does not establish command-schema compatibility. The SDK Git
revision is pinned in `Cargo.toml`/`Cargo.lock` and recorded in every archive.
See the [SDK command contract](https://github.com/oai404iao/ai_gateway/blob/connector-sdk-v0.1.0/crates/connector-sdk/docs/commands.md)
for required common commands and the separately version-coupled Codex control
adapter; the [SDK example](https://github.com/oai404iao/ai_gateway/blob/connector-sdk-v0.1.0/crates/connector-sdk/examples/responses.rs)
shows a routable generic connector without Codex lifecycle privileges.
Linux GNU builds require a compatible glibc and native architecture; do not load
them on musl/Alpine, Windows, or macOS.

## Build and test

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/package-release.sh 0.1.0
```

The packaging script builds the real shared library, reads its exported ABI
descriptor, and verifies an archive containing the library, manifest,
build metadata, checksums, licenses, and documentation. No provider credentials
or paid upstream calls are required.

See [installation and upgrades](docs/deployment.md) and
[release maintenance](docs/releasing.md). There is no auto-download, hot reload,
or built-in Codex fallback.

## Security and license

Plugins execute trusted native code with the gateway's privileges. SHA pinning
verifies the configured bytes; it does not sandbox the module. Only install
reviewed artifacts from a trusted source.

This project is [AGPL-3.0-only](LICENSE). Redistribution must retain the license,
dependency notices, and applicable corresponding-source obligations. Archives
identify the exact source revision and include third-party license texts.
