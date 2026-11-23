# Codex protocol

## Compatibility

The native connector identifies itself as `codex`. Use a gateway with ABI 1 and command protocol 3 that takes operation capabilities from `attempt.describe/v1`. The plugin receives compiled settings in `metadata.settings` and supplies an opaque request context. A matching CPU architecture or ABI alone does not establish command compatibility.

The exported manifest lists supported commands and operations; release packages ship a copy as `manifest.json`. Provider settings use [schema 1](configuration.md). Package metadata and SDK provenance are described in [Development](development.md#archive-contents).

## Supported operations

These are connector operation IDs for gateway routing, not additional HTTP endpoints. Each has one transport:

| Operation | Transport | Request |
| --- | --- | --- |
| `responses` | `sse` | HTTP Responses with `stream: true` and `store: false` |
| `responses-ws` | `websocket` | Sequential `response.create`; continuation requires the same eligible upstream connection |
| `web_search` | `non_stream` | Standalone search with its own request fields and route |
| `images_generation` | `non_stream` | JSON generation request |
| `images_edit` | `non_stream` | Gateway captures client multipart data and streams the plugin's JSON/base64 edit format |

Responses HTTP and WebSocket strip caller-supplied `x-codex-routing-hint`; the gateway can generate the hint from the final wire model and tier. Both accept `prompt_cache_options` with exactly `{"ttl":"30m"}` or `{"ttl":"12h"}`; `null` is omitted.

### Image restrictions

- Generation accepts `model`, `prompt`, `background`, `n`, `quality`, and `size`. Edit requires `model`, `prompt` and one to five input images, with no mask or duplicate text fields. `input_fidelity` and `output_compression` are unsupported.
- For an edit, `background` accepts `transparent`, `opaque`, or `auto`; `quality` accepts `low`, `medium`, `high`, or `auto`. `n` is an unsigned integer. An edit's `stream` must be `false` or `0` if present.
- Generation and edit accept `output_format=gif` with optional `frame_count` (2–16) and `frame_duration_ms` (20–2000). These are sibling integers, sent as decimal text in multipart; neither is allowed for other formats. `output_format=png` is omitted.
- Edit image parts need an `image/` content type of at most 128 bytes, or a `.png`, `.jpg`, `.jpeg`, `.webp`, or `.gif` filename if the type is missing. The gateway enforces upload size limits.

### Operation descriptors and usage

`attempt.describe/v1` takes an operation and returns its capability flags, supported transport, response mode, and usage descriptor. All Codex operations return provider responses unchanged and declare `{"parser":"general","format":"open_ai_responses"}` for usage. The gateway parses the actual upstream counters; the connector does not estimate missing usage or set prices.

## OAuth, models, and quotas

`endpoints` provides the issuer and Responses base URL. `authorize_url` constructs the PKCE S256 authorization URL; the redirect used for authorization and token exchange is `http://localhost:1455/auth/callback`. `parse_callback` checks the loopback scheme, host, port and path, and extracts a nonempty code and state. Complete authorization through the gateway using that callback. Restart an unfinished flow if the plugin generation changes.

The connector plans token exchange, refresh, model discovery, quota reads and reset-credit consumption; the gateway executes the plans. A refresh may update a subset of tokens but must return at least one nonempty token. Permanent refresh failures require reauthorization. A failed backend request is not by itself a reason to replace credentials. Quota-window observations are account data, not billable request usage.
