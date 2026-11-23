# Working on the native Codex connector

Start with the [source layout and local checks](docs/development.md). This repository prepares requests and parses provider results. HTTP transport, credential persistence, routing, accounting and logs belong in the gateway.

- When changing commands, keep the exported manifest, dispatch, [host-facing contract](docs/codex.md) and native package verification in sync. Native ABI, command protocol and settings schema are independent versions. Do not silently change a published tag or let a release depend on an unpinned SDK.
- Read configured identity from `metadata.settings` and preserve credential-scoped installation IDs. Outbound policy and privacy normalization belong in this plugin. Do not restore a field already removed by gateway policy or put provider tokens in diagnostics.
- Keep all Codex operations pass-through; use the gateway's declared general usage parser for upstream Responses counters rather than introducing plugin billing or estimation.
- Run the pinned format, lint, workspace and packaging unit checks from the repository root. For native changes run `./scripts/verify-release.sh VERSION --development` with the workspace version on supported GNU Linux. Checks need neither a provider credential nor a live upstream call.
- Write documentation in English under `docs/`; keep [the index](docs/index.md) linked to task guides. Preserve released changelog entries and license materials. Follow [release maintenance](docs/releasing.md) only when preparing a release; packaging does not authorize publication or activation.
