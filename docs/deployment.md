# Install the Codex connector

> Status: Current development deployment contract for connector 0.2 protocol 3; requires the matching gateway implementation.

## Verify an artifact

Obtain the matching architecture archive and its `.sha256` file from a trusted
[release](https://github.com/oai404iao/ai-gateway-connectors/releases). Review its
source revision before trusting checksums from that same source.

```sh
sha256sum -c ai-gateway-connector-codex-0.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

The archive contains the library, exported manifest, build information,
checksums, and redistribution licenses. `build-info.json` records the ABI,
target, libc baseline, library digest, source revision, and SDK source.
Installation does not create credentials, grant access, or enable routes.

## Directory and installation

The gateway TOML selects one managed, persistent directory:

```toml
[plugins]
directory = "/var/lib/ai-gateway/plugins"
```

Install a verified package using the gateway's administrator plugin-management
page or its documented local incoming-directory workflow. Both use the same
bounded package validation and immutable artifact store. Newly installed modules
remain disabled until explicitly enabled. Do not overwrite an active `.so`.

The gateway process must be able to manage the directory, while other users
must not be able to write it. Keep ownership valid inside containers, use a
persistent volume, and retain package licenses/notices. Unlike the old static
TOML loader, a read-only plugin-directory mount cannot support web installation.
Plugins remain separate from the standard gateway image.

Official GNU artifacts target Debian bookworm's libc baseline and the matching
CPU architecture; they are not musl/Alpine artifacts.

## Settings and activation

The management page renders the plugin's localized settings descriptor.
Configure client version, originator, User-Agent, synthetic workspace, and Git
origin there—not in the gateway's system-settings document.

The plugin validates its own values and compiles the immutable configuration.
The gateway persists complete values and uses optimistic concurrency for saves.
Changing plugin defaults does not silently change an existing saved document.
Historical gateway settings must be migrated by the matching gateway upgrade.

Enable a validated artifact explicitly. Missing, disabled, incompatible, or
invalid modules make their routes unavailable without substituting `general`.
Already loaded code is trusted native code even when business dispatch is disabled.

Protocol 3 requires a gateway accepting Codex's explicit `attempt.describe/v1`
transport and upstream-usage declarations. Responses HTTP supports SSE only;
WebSocket, Search and Images retain their operation-specific transports.
All response modes are pass-through, with the gateway's `general` usage parser
selected for the actual upstream Responses counter interface. Upgrading does
not enable response rewriting or change the five-field settings schema.

## Compatible upgrades and rollback

1. Retain the previous artifact, complete settings, and consistent database/spool backup.
2. Install the new package without replacing the previous file.
3. Validate command and settings-schema compatibility.
4. Activate the new immutable generation; new operations use it while in-flight operations finish on their pinned generation.
5. Roll back only when the previous code accepts the current persisted state and settings.

The gateway does not physically unload library code in a running process.
Repeated artifact loads are bounded and may eventually require a maintenance
restart; settings-only updates reuse the same library.

WebSocket connections do not cross plugin generations. Clients must recover
from lost continuation state rather than move it to another upstream socket.
Unfinished OAuth flows from another generation must restart authorization.
Already-dispatched token/financial operations retain their durable recovery
rules; enabling, disabling, upgrading, or rolling back never clears an intent.
Incompatible persistent-state migrations are not promised as zero-downtime upgrades.

## Trust and native dependencies

Installation and activation are privileged code-management operations, not
ordinary business configuration. The loader verifies a frozen in-memory image
before execution; a digest identifies approved bytes but does not sandbox them.
Native code can access process credentials, crash, or hang the gateway.

The plugin uses gateway transport, not an independent HTTP runtime. Transitive
native dependencies must also be trusted; a library digest does not cover them.
Avoid `$ORIGIN`-relative dependencies because loading uses a sealed file image
rather than the original installation pathname.
