# Configure Codex

Configure Codex through the gateway's plugin-management page. These values belong
to the plugin, not the gateway's system-settings document. The page uses the
connector's localized settings descriptor and materialized defaults.

## Settings

Settings schema 1 contains exactly five required string fields:

| Field | Default | Purpose | Maximum characters |
| --- | --- | --- | --- |
| `workspace_path` | `/workspace` | Synthetic workspace path in provider metadata | 1024 |
| `git_remote_url` | `https://github.com/oai404iao/ai_gateway` | Synthetic Git origin in provider metadata | 2048 |
| `originator` | `codex_cli_rs` | Provider originator header | 256 |
| `client_version` | `0.146.0` | Provider client-version identity | 128 |
| `user_agent` | `codex_cli_rs/0.146.0` | Provider User-Agent header | 1024 |

The workspace and Git origin are synthetic identity values, not instructions to
open a local directory or clone a repository. They do not configure gateway
storage or a real checkout.

The complete default values document is:

```json
{
  "workspace_path": "/workspace",
  "git_remote_url": "https://github.com/oai404iao/ai_gateway",
  "originator": "codex_cli_rs",
  "client_version": "0.146.0",
  "user_agent": "codex_cli_rs/0.146.0"
}
```

This illustrates the plugin values, not a standalone configuration file to place
beside the library. Credentials, models, routes, prices, transport limits, and
the managed plugin directory are configured in the gateway.

## Validation

- All five fields must be present as strings. Unknown fields and unsupported
  schema versions are rejected.
- `workspace_path` must start with `/`, contain no control characters, and have
  no leading or trailing whitespace.
- `git_remote_url` must be an HTTPS URL with a host and a non-root path. User
  information, query strings, fragments, and surrounding whitespace are rejected.
- `originator`, `client_version`, and `user_agent` must be nonempty HTTP header
  values without surrounding whitespace.
- Each value must fit its field's character limit.

Changing `client_version` does not derive a new `user_agent`; set both explicitly
when you intend them to match.

## Save and activate settings

The plugin validates the values and compiles an immutable configuration. The
gateway persists complete values, uses optimistic concurrency for saves, and
supplies the compiled configuration through reserved `metadata.settings`.
Request and maintenance commands use that configuration, not legacy identity
fields interpreted by the gateway.

Existing saved values do not silently change when a later plugin changes its
defaults. Schema 1 migration only accepts already-valid schema-1 values; it is
not an automatic converter for older gateway system settings. Historical
gateway settings require the matching gateway upgrade's migration.

For generation activation and rollback constraints, see
[installation and upgrades](deployment.md#upgrade-or-roll-back).

## Request identity and privacy

`attempt.context` derives opaque identity from the host-selected credential ID,
request ID, optional affinity hash, and captured client headers. The gateway
treats the result as opaque. Installation IDs are credential-scoped and preserve
the pre-split UUID algorithm across restarts, token refresh, and upgrades.

The connector owns outbound body/header allowlists and normalizes Codex
fingerprints using the configured synthetic workspace and Git origin. The policy
is embedded from
[`codex/src/request-allowlists.json`](https://github.com/oai404iao/ai-gateway-connectors/blob/main/codex/src/request-allowlists.json).
Unlisted caller headers are not forwarded. Unknown body keys are rejected;
operation-specific ignored fields must still contain accepted values. Responses
HTTP forces `stream: true` and `store: false`; WebSocket adaptation also sets
`type: "response.create"`.

Responses body metadata and operation headers have distinct normalization
paths. Do not assume arbitrary body content is scrubbed across every operation.
Provider authentication and selected model/credential binding remain under
gateway control; filtering must not restore metadata already removed by gateway
billing policy.

Privacy normalization is not general content anonymization. Prompts, image data,
and other permitted request content still go to the provider. The gateway retains
public ingress policy, request logging, transport limits, and financial
enforcement. Review those settings separately when handling sensitive data.
