# Install the Codex connector

Install Codex separately from the gateway, then configure and enable it through
gateway administration. Installation does not create credentials, grant access,
or enable routes.

> Status: Current development deployment contract for protocol 3. Use a matching
> gateway implementation and check the manifest shipped with your artifact.

## 1. Check compatibility

Confirm [the gateway contract and platform](codex.md#compatibility) before
downloading. Official artifacts support GNU Linux on x86_64 or aarch64, with
glibc requirements no newer than 2.36. They are not musl/Alpine builds.

A matching ABI alone is insufficient: the gateway must support the artifact's
command protocol and plugin lifecycle. Do not assume an archive named `0.2.0`
implements the unreleased protocol-3 contract.

## 2. Verify an artifact

Obtain the archive for your architecture and its `.sha256` file from a trusted
[release](https://github.com/oai404iao/ai-gateway-connectors/releases). Check the
source revision and release compatibility before installing it. A checksum from
the same download source does not independently establish trust.

For example, with both files in the current directory:

```sh
sha256sum -c ai-gateway-connector-codex-0.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

Replace the version and target with those of your chosen release. The archive
contains the library, manifest, build metadata, checksums, documentation, and
redistribution licenses. See [archive contents](development.md#archive-contents).

## 3. Install into the managed directory

The gateway TOML selects one managed, persistent directory:

```toml
[plugins]
directory = "/var/lib/ai-gateway/plugins"
```

Use the gateway's administrator plugin-management page or its documented local
incoming-directory workflow to install the verified package. Both use bounded
package validation and an immutable artifact store. Follow the matching gateway
documentation for the exact upload or incoming-directory procedure; this
repository does not supply a separate install command.

The gateway process must be able to manage this directory. Other users must not
be able to write it. In containers, keep ownership valid for the gateway's runtime
identity and use a persistent volume. A read-only mount cannot support web
installation. Plugins remain separate from the standard gateway image.

Do not overwrite an active `.so`. Keep the package's licenses and notices with
the installed artifact.

## 4. Configure and enable

Newly installed modules remain disabled. Set the five [Codex settings](configuration.md)
on the plugin-management page, let the plugin validate and compile them, then
explicitly enable a validated artifact.

Use the gateway's normal credential and route management to make the enabled
connector available to clients. Missing, disabled, incompatible, or invalid
modules make their routes unavailable; the gateway does not substitute `general`.

Current protocol 3 declares SSE-only Responses HTTP, WebSocket Responses, and
non-streaming Search/Images, all with pass-through responses. It does not add
response rewriting or change the five-field settings schema. See
[supported operations](codex.md#supported-operations).

## Upgrade or roll back

There is no automatic connector upgrade.

1. Retain the previous artifact, complete settings, and a consistent gateway
   database/spool backup.
2. Install the new package alongside the old artifact; do not replace its file.
3. Validate the command contract and settings-schema compatibility.
4. Activate the new immutable generation. New operations use it; in-flight
   operations finish on their pinned generation.
5. Roll back only if the previous code accepts the current persisted state and
   settings.

The gateway does not physically unload native libraries in a running process.
Repeated artifact loads are bounded and may eventually require a maintenance
restart. Settings-only updates reuse the same library. Disabling business
dispatch does not undo the execution of already-loaded native code.

WebSocket connections cannot cross generations. Clients must recover from lost
continuation state rather than move it to another upstream socket. Unfinished
OAuth flows from another generation must restart authorization.

Already-dispatched token and financial operations retain their durable recovery
rules. Enabling, disabling, upgrading, and rollback never clear an intent.
Incompatible persistent-state migrations are not promised as zero-downtime
upgrades.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Package rejected before activation | Archive checksums, architecture, libc baseline, ABI, command protocol, and matching gateway lifecycle support |
| Installation cannot write files | Managed-directory ownership, container runtime identity, and whether the persistent mount is writable |
| Installed connector has no available route | Explicit enablement, valid settings, and gateway credential/route configuration |
| Settings rejected | All five required fields, schema 1, field limits, and the [validation rules](configuration.md#validation) |
| Non-streaming Responses or streaming Search/Images fail | Use the operation's [supported transport](codex.md#supported-operations) |
| Authorization or WebSocket continuation fails after activation | Restart the unfinished authorization or recover the client connection; do not reuse state from another generation |
| Further library loads reach a limit | Plan a maintenance restart; do not overwrite a loaded library to bypass the bound |

Gateway transport, credential persistence, billing, and logging problems must be
diagnosed in the gateway; the connector has no separate service to restart.

## Trust and native dependencies

Installation and activation are privileged code-management operations. The
loader verifies a frozen in-memory image before execution, but a library digest
is not a sandbox. Native code can access process credentials, crash, or hang the
gateway. Install only reviewed artifacts from a trusted source.

The plugin uses gateway transport, not an independent HTTP runtime. Transitive
native dependencies must also be trusted; the library digest does not cover
them. Avoid `$ORIGIN`-relative dependencies because loading uses a sealed file
image rather than the original installation pathname.
