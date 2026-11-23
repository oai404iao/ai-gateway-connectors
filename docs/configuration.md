# Configure Codex

In the gateway's **Plugin management** page, select Codex and edit its settings. These values set the identity shown to the provider; the paths and Git origin do not refer to a checkout the gateway opens.

## Settings

Settings schema 1 requires five strings:

| Field | Default | Meaning | Maximum characters |
| --- | --- | --- | ---: |
| `workspace_path` | `/workspace` | Synthetic workspace path | 1024 |
| `git_remote_url` | `https://github.com/oai404iao/ai_gateway` | Synthetic Git origin | 2048 |
| `originator` | `codex_cli_rs` | Provider originator header | 256 |
| `client_version` | `0.146.0` | Client version header and model discovery query | 128 |
| `user_agent` | `codex_cli_rs/0.146.0` | Provider User-Agent header | 1024 |

## Validation

Use an absolute workspace path with no control characters. The Git origin must be an HTTPS URL with a host and a non-root path, without user information, query or fragment. Both must have no surrounding whitespace. The three header values require nonempty visible ASCII without controls or surrounding whitespace. Set the version and User-Agent separately if both should change.

Saving validates the complete document and publishes a new generation for subsequent operations. Existing operations keep the configuration they acquired. The plugin accepts schema 1; a different saved schema must be handled before activation.

## What the provider sees

The connector replaces client installation IDs with a credential-scoped identifier and replaces turn-metadata workspaces with the configured synthetic path and Git origin. Eligible session and thread identifiers are preserved; missing ones are filled from request or affinity context. The [outbound policy](https://github.com/oai404iao/ai-gateway-connectors/blob/main/codex/src/request-allowlists.json) controls permitted headers and fields for each operation.

These substitutions apply to request metadata. Prompt text, image data and other permitted content still reach the provider. Credentials, routing and billing policy are managed separately in the gateway.
