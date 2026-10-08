# Install the Codex connector

> Status: Current Linux administrator installation procedure.

## Verify an artifact

Download the matching target archive and its `.sha256` file from a trusted
[release](https://github.com/oai404iao/ai-gateway-connectors/releases). Review the
release/tag and source revision before trusting checksums from the same source.

```sh
sha256sum -c ai-gateway-connector-codex-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf ai-gateway-connector-codex-0.1.0-x86_64-unknown-linux-gnu.tar.gz
cd ai-gateway-connector-codex-0.1.0-x86_64-unknown-linux-gnu
sha256sum -c SHA256SUMS
```

`manifest.json` is the module's exported manifest, not an installer.
`build-info.json` records ABI, build target, GNU libc baseline, library digest, source revision,
and SDK source. This package does not configure identities or grant user access.

## Install and pin

The library and every parent directory must be owned by root or the gateway
effective user. Parent directories must not be group/other writable or symlinks.
The library must be a regular file with no write permission bits.

```sh
sudo install -d -o root -g root -m 0755 /opt/ai-gateway/plugins/codex
sudo install -o root -g root -m 0444 libai_gateway_connector_codex.so \
  /opt/ai-gateway/plugins/codex/libai_gateway_connector_codex.so
sha256sum /opt/ai-gateway/plugins/codex/libai_gateway_connector_codex.so
```

Add this startup configuration to the gateway TOML, replacing the digest with
the **library file** SHA-256, not the archive digest:

```toml
[[plugins]]
id = "codex"
path = "/opt/ai-gateway/plugins/codex/libai_gateway_connector_codex.so"
sha256 = "REPLACE_WITH_64_HEXADECIMAL_DIGITS"
```

Restart the gateway. Missing libraries, wrong hashes, unsupported platforms,
unsafe paths, duplicate/reserved IDs, incompatible ABIs, and mismatched manifests
fail closed. Persisted Codex routes require the installed Codex module; the
gateway never substitutes `general`.

For containers, mount the protected directory read-only at the configured
absolute path. Keep the configured UID ownership rule valid inside the container.
The plugin is not part of the standard gateway image; provision the mount before
startup. Retain the package's licenses/notices with redistributed installations.
Official artifacts target Debian bookworm's GNU libc 2.36 baseline and the
matching CPU architecture; they can load in the official gateway image without
requiring Ubuntu 24.04's newer libc. They are not musl/Alpine artifacts.

## Upgrade and rollback

1. Back up the existing TOML pin and installed artifact; retain its source revision.
2. Verify compatibility and the new package checksum before maintenance.
3. Stop the gateway, install the replacement with mode `0444`, update its pin, restart.
4. Check startup and operator-visible connector availability before restoring traffic.
5. Roll back by restoring **both** the previous artifact and digest, then restart.

The process holds already-loaded code until exit. Changing a file/config on disk
does not replace code in a running process. Removal also requires restart and
removing/disabling persisted routes which require that connector.

## Trust and native dependencies

The loader verifies a sealed in-memory copy before executing the module. This
prevents path replacement between hashing and loading, but not malicious code
inside an approved artifact. Native code can access credentials and all process
privileges; crashes and hangs are not contained.

The module uses gateway transport rather than bringing an independent HTTP
runtime. Native system dependencies must still be present and trusted. The
library pin does not cover transitive shared libraries. Avoid `$ORIGIN`-relative
dependencies: the gateway loads a sealed `/proc/self/fd` image, not the original
installation pathname.
