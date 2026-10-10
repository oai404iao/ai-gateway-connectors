# ai-gateway-connectors

> Status: Current external connector implementation.

Native provider connectors for [ai-gateway](https://github.com/oai404iao/ai_gateway).
The gateway retains its built-in `general` connector. **Codex is independently
built, released, and installed from this repository; it is not bundled in
gateway binaries or images.**

The Codex module owns its settings, credential-stable installation IDs, privacy
normalization, outbound body/Header policy, OAuth protocol planning/parsing,
model and quota discovery, and provider adaptation for Responses HTTP/WebSocket,
standalone web search, and Images generation/edit. Gateway-managed HTTP transport, durable
credentials, routing, billing, and request logging remain gateway responsibilities.

## Compatibility

| Contract | Value |
| --- | --- |
| Connector ID | `codex` |
| Native SDK ABI | `1` |
| Manifest protocol version | `3` (required; incompatible hosts reject before dispatch) |
| Release build-info schema | `1` |
| Command schema | Codex configured-generation contract 3 with explicit transport/usage descriptors |
| Settings schema | `1` (five provider-owned scalar fields) |
| Library | `libai_gateway_connector_codex.so` |
| Release targets | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Build baseline | Pinned Debian bookworm Rust 1.97.1 container, GNU libc 2.36 or older requirements |

Use a gateway version built against the same SDK ABI and Codex command schema.
An ABI match alone does not establish command-schema compatibility. The SDK Git
revision is pinned in `Cargo.toml`/`Cargo.lock` and recorded in every archive.
See the [SDK command contract](https://github.com/oai404iao/ai_gateway/blob/87892257453a11be128afa4f5ba863c93e9453f2/crates/connector-sdk/docs/commands.md)
for the C ABI and original configured-generation contract. The pinned SDK's
ABI-1 JSON envelopes also encode the protocol-3 descriptors below without a
dependency revision change. This connector requires a gateway supporting those
descriptors and the plugin-lifecycle settings/context contract. It is not compatible with the
startup-only gateway's identity-metadata protocol. The [SDK example](https://github.com/oai404iao/ai_gateway/blob/87892257453a11be128afa4f5ba863c93e9453f2/crates/connector-sdk/examples/responses.rs)
shows a routable generic connector without Codex lifecycle privileges.
Linux GNU builds require a compatible glibc and native architecture; do not load
them on musl/Alpine, Windows, or macOS.

`attempt.describe/v1` is a pure, bounded per-operation declaration with all three
legacy capability flags, one supported protocol and response mode `passthrough`:

| Operation | Protocol |
| --- | --- |
| `responses` | `sse` |
| `responses-ws` | `websocket` |
| `web_search`, `images_generation`, `images_edit` | `non_stream` |

The gateway maps Images edit's non-streaming descriptor to multipart transport.
Responses HTTP is streaming-only; the stronger descriptor allows the gateway
to reject unsupported non-streaming requests before upstream dispatch.
There are no `response.json/v1` or `response.event/v1` commands: provider response
bytes, usage and terminal events remain unmodified.

Every descriptor declares
`"usage":{"parser":"general","format":"open_ai_responses"}`. This selects the
gateway's shared parser by the **actual upstream usage interface**, including
Codex Images and standalone search, rather than assuming the public client
format determines provider counters. It does not estimate missing usage,
change billing, parse OAuth quota windows, or give the plugin transport/database
ownership. Canonical storage, input/cache token inclusion rules and prices remain
gateway responsibilities.

## Build and test

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/package-release.sh 0.2.0
```

The packaging script builds the real shared library, reads its exported ABI
descriptor, and verifies an archive containing the library, manifest,
build metadata, checksums, licenses, and documentation. No provider credentials
or paid upstream calls are required.

See [installation and upgrades](docs/deployment.md) and
[release maintenance](docs/releasing.md). Compatible gateways support directory
discovery and generation-based hot activation/upgrades; installation alone does
not enable a module. There is no automatic upgrade or built-in Codex fallback.

## Settings and request privacy

`settings.describe/v1` supplies localized field descriptions and materialized
defaults. `settings.validate/v1` checks the five fields; `settings.compile/v1`
returns the immutable configuration consumed through the reserved
`metadata.settings` envelope. Unknown fields and unsupported schema migrations
fail closed. Version, originator, User-Agent, synthetic workspace path, and
synthetic Git origin are not gateway system settings.

`attempt.context` derives opaque request identity from the host-selected
credential ID, request ID, optional affinity hash, and captured client headers.
The gateway treats its result as opaque. The installation ID algorithm preserves
the pre-split credential-scoped UUID across restarts, token refresh, and upgrades.
The embedded `codex/src/request-allowlists.json` owns provider outbound policy
and privacy normalization. The gateway retains public ingress policy, selected
model/credential binding, transport limits, and financial enforcement.

## Security and license

Plugins execute trusted native code with the gateway's privileges. SHA pinning
verifies the configured bytes; it does not sandbox the module. Only install
reviewed artifacts from a trusted source.

This project is [AGPL-3.0-only](LICENSE). Redistribution must retain the license,
dependency notices, and applicable corresponding-source obligations. Archives
identify the exact source revision and include third-party license texts.
