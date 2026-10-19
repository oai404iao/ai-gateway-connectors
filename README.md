# ai-gateway-connectors

Native provider connectors for [ai-gateway](https://github.com/oai404iao/ai_gateway).

The gateway includes its own `general` connector. This repository supplies
**Codex as a separate native plugin**: build, release, install, and enable it
independently. Gateway binaries and images do not bundle it, and there is no
automatic installation, upgrade, or built-in Codex fallback.

Codex handles provider settings, request identity and privacy, OAuth protocol
adaptation, model and quota discovery, Responses HTTP/WebSocket, standalone
search, and Images generation/edit. The gateway owns HTTP transport, durable
credentials, routing, billing, and request logging.

## Getting started

> Status: Current source requires Codex command protocol 3 and a matching
> gateway. The published 0.2.0 history records protocol 2; do not infer protocol
> compatibility from the package version or ABI alone.

1. Check the gateway and platform against [Codex compatibility](docs/codex.md#compatibility).
2. Obtain a trusted archive for your architecture and [verify and install it](docs/deployment.md).
3. Configure the five [plugin settings](docs/configuration.md) in gateway management.
4. Explicitly enable the artifact, then use the gateway to configure credentials
   and routes. Installation alone does not enable requests.

See the [documentation](docs/index.md) for supported operations, upgrades,
troubleshooting, and development.

## Development

From the repository root, with the toolchain in `rust-toolchain.toml`:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

See [development](docs/development.md) for prerequisites and package checks,
[release maintenance](docs/releasing.md) for publishing, and
[AGENTS.md](https://github.com/oai404iao/ai-gateway-connectors/blob/main/AGENTS.md)
for repository-specific agent guidance.

## Security and license

Plugins run trusted native code with the gateway's privileges. Checksums identify
bytes; they do not sandbox code. Review the [trust boundary](docs/deployment.md#trust-and-native-dependencies)
before installing an artifact.

This project is [AGPL-3.0-only](LICENSE). Redistribution must retain license and
dependency notices and meet applicable corresponding-source obligations.
Release archives identify their source revision and include third-party license
texts. See the [changelog](CHANGELOG.md) for release history.
