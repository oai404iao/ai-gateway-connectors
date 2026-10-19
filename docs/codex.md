# Codex operations and compatibility

Codex adapts gateway operations to the provider's protocol. It does not run its
own HTTP service, store credentials, or replace gateway routing and billing.
Install and enable it through a compatible gateway before configuring routes.

## Compatibility

> Status: Current development contract. Protocol 3 support is required in the
> gateway; the 0.2.0 changelog describes the earlier protocol-2 release.

| Contract | Current source |
| --- | --- |
| Connector ID | `codex` |
| Native SDK ABI | `1` |
| Manifest command protocol | `3` |
| Settings schema | `1` |
| Release build-info schema | `1` |
| Library | `libai_gateway_connector_codex.so` |
| Release targets | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Official build baseline | Digest-pinned Debian bookworm Rust 1.97.1 container; glibc requirements no newer than 2.36 |

Use the matching native CPU architecture and a compatible GNU libc. Official
artifacts do not support musl/Alpine, Windows, or macOS.

The gateway must support both ABI 1 and Codex's protocol-3 configured-generation
contract, including plugin-owned settings, opaque request context, explicit
transport descriptors, and upstream-usage declarations. An ABI match is not
enough. Startup-only hosts using the old identity-metadata contract are not
compatible. Incompatible hosts must reject the manifest before dispatch.

The SDK Git revision is pinned in `Cargo.toml` and `Cargo.lock` and recorded in
each archive's `build-info.json`. The reviewed pin's ABI-1 JSON envelopes can
carry protocol-3 descriptors without a dependency revision change. Check the
artifact's manifest and the host's command support, not just the SDK revision.

## Supported operations

Use the gateway's client API and route configuration. The names below are
connector operation identifiers, not additional public HTTP endpoints.

| Operation | Declared protocol | Behavior |
| --- | --- | --- |
| `responses` | `sse` | Streaming Responses over HTTP; non-streaming requests are unsupported |
| `responses-ws` | `websocket` | Responses over a WebSocket connection |
| `web_search` | `non_stream` | Standalone search |
| `images_generation` | `non_stream` | Image generation; streaming is unsupported |
| `images_edit` | `non_stream` | Image edit; the gateway handles client multipart input |

For image edits, the connector validates fields and supplies JSON/base64 body
templates; the gateway handles image bytes and transport. See the gateway's
upload limits as well as the connector's restrictions. A supported operation
does not guarantee that a particular model or credential has provider access.

### Image restrictions

- Image edits support at most five images and do not support masks.
- Generation accepts `background`, `model`, `n`, `prompt`, `quality`, and `size`.
  Edit accepts the corresponding multipart fields plus `image` or `image[]`;
  `model` and `prompt` are required. Duplicate edit text fields are rejected.
- Edit `background` must be `transparent`, `opaque`, or `auto`; `quality` must be
  `low`, `medium`, `high`, or `auto`. `n` must parse as an unsigned integer.
- Image streaming is unsupported. An edit `stream` field is accepted only as
  `false` or `0` and is not forwarded.
- Edit fields `input_fidelity`, `mask`, and `output_compression` are rejected.
- Image parts need an `image/` content type of at most 128 bytes, or a recognized
  `.png`, `.jpg`, `.jpeg`, `.webp`, or `.gif` filename when the type is absent.

These are connector policy limits, not a promise of full Images API parity.

### WebSocket continuation

WebSocket connections stay on their selected plugin generation. Continuation
state cannot be moved to another upstream socket; clients must recover when a
connection is lost.

## OAuth, models, and quotas

Use the gateway's credential-management workflow for authorization and refresh.
The connector plans and parses OAuth exchanges; the gateway performs network
requests and stores credentials durably. The connector does not require its own
credential file or background login process.

Authorization uses PKCE S256 and the loopback redirect
`http://localhost:1455/auth/callback`. The callback must use that exact scheme,
host, port, and path and include nonempty `code` and `state`. Use the matching
gateway's procedure for completing authorization on a remote machine rather
than changing the callback URL.

Model and quota discovery follow the same division: Codex supplies request plans
and parses provider results, while the gateway schedules and executes them.
Quota observations and reset information are provider account data, not billing
usage. Restart authorization if an unfinished OAuth flow belongs to a different
plugin generation.

If authorization fails, distinguish a denied or malformed callback from a token
endpoint or backend error. An invalid or expired refresh token may require
reauthorization; generic backend failures or malformed model/quota data should
be diagnosed as such, not treated automatically as a reason to log in again.

## Responses and usage

Every operation declares response mode `passthrough`. Codex does not implement
`response.json/v1` or `response.event/v1`; it does not rewrite provider response
bytes, usage fields, or terminal events.

All operation descriptors select:

```json
{"usage":{"parser":"general","format":"open_ai_responses"}}
```

This tells the gateway to use its shared parser for the actual upstream
Responses counter interface, including standalone search and Images. The public
client API format does not determine which upstream usage parser to use.

The descriptor does not estimate missing usage, parse OAuth quota windows,
change prices, or transfer billing ownership to the plugin. Canonical usage
storage and input/cache token inclusion rules remain gateway responsibilities.

## Connector command surface

These are native SDK commands for host integration, not shell commands:

| Area | Commands |
| --- | --- |
| Settings | `settings.describe/v1`, `settings.validate/v1`, `settings.compile/v1`, `settings.migrate/v1` |
| OAuth | `endpoints`, `authorize_url`, `parse_callback`, `parse_identity`, `parse_expiration`, `exchange_plan`, `exchange_parse`, `refresh_plan`, `refresh_parse` |
| Discovery | `models_plan`, `models_parse`, `quota_plan`, `quota_parse`, `quota_reset_plan`, `quota_reset_parse` |
| Request adaptation | `attempt.context`, `attempt.body`, `attempt.target`, `attempt.headers`, `attempt.capabilities`, `attempt.describe/v1` |
| Image edit | `attempt.image_edit_plan`, `attempt.image_part_plan` |

`attempt.describe/v1` is a pure, bounded declaration for one operation. It returns
the three capability flags (`preserves_affinity_on_failure`,
`successful_response_is_sse`, `changes_request_body`), one supported protocol,
response pass-through, and the usage descriptor. The gateway uses it to reject
unsupported transports before upstream dispatch. Images edit's non-streaming
descriptor maps to the gateway's multipart handling.

For implementation details, start with the exported manifest in
[`codex/src/lib.rs`](https://github.com/oai404iao/ai-gateway-connectors/blob/main/codex/src/lib.rs)
and the SDK revision recorded with your artifact.
