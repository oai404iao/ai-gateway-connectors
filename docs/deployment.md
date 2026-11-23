# Install the Codex connector

Install through ai-gateway's plugin management. You need administrator access, a gateway supporting the [current contract](codex.md#compatibility), and a trusted archive for the matching GNU Linux target (`x86_64` or `aarch64`). Official builds use Debian bookworm and require glibc symbols no newer than 2.36.

## Verify and install

1. Download the matching archive and `.sha256` file from the [connector release](https://github.com/oai404iao/ai-gateway-connectors/releases). Run `sha256sum -c <archive>.sha256` in that directory. This checks the bytes against the checksum, not the trustworthiness of the release source.
2. In gateway TOML, set `[plugins].directory` to an absolute path on persistent storage, for example `/var/lib/ai-gateway/plugins`. The gateway process needs write access and exclusive use of this directory.
3. In **Plugin management**, upload the `.tar.gz`, confirm with your administrator password and wait for installation to finish. For deployment by file delivery, use the gateway's [incoming-directory procedure](https://github.com/oai404iao/ai_gateway/blob/main/docs/user/connector-plugins.md).
4. Select the installed artifact, enable it and review its [settings](configuration.md). Then connect a Codex account, bind a logical channel to that credential, and route the desired operations. Clients still need the gateway's channel authorization.

## Upgrade or roll back

Keep the previous artifact and settings, and back up the gateway database and request spool at a consistent point. Install the new package alongside the installed version and explicitly select it. New requests use the new generation; in-flight requests complete on their pinned generation. A WebSocket continuation cannot move to another upstream socket. If context is lost, send a full request. An unfinished OAuth flow may need to restart.

Roll back only when the selected artifact can read the stored settings and state. Changing versions does not clear pending token or financial operations. The gateway retains loaded libraries until process exit. If the load limit is reached, schedule a restart rather than replacing an installed `.so` in place.

## Trust and native dependencies

Native code runs with the gateway process's privileges. Restrict installation and access to the managed directory accordingly. The loader verifies and freezes the library image; this does not sandbox execution or authenticate its transitive native dependencies. Avoid `$ORIGIN`-relative dependencies: the loader does not run the library from its original package path.
