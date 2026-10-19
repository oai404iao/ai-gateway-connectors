# Documentation

ai-gateway-connectors provides independently installed native connectors for
ai-gateway. The current connector is `codex`; this is not a standalone proxy or
client application.

## Start using Codex

- [Codex operations and compatibility](codex.md): supported transports, gateway
  requirements, OAuth, model discovery, quotas, and usage accounting.
- [Install the Codex connector](deployment.md): verify an archive, install it,
  activate it, and troubleshoot unavailable routes.
- [Configure Codex](configuration.md): the five provider settings, validation,
  request identity, and privacy normalization.

For gateway credential and route setup, use the documentation for your matching
[ai-gateway](https://github.com/oai404iao/ai_gateway) version. This repository
defines the connector side of that integration, not a separate management API.

## Maintain an installation

- [Upgrade or roll back](deployment.md#upgrade-or-roll-back) without replacing an
  active library or moving in-flight work between generations.
- [Understand the native-code trust boundary](deployment.md#trust-and-native-dependencies).
- [Read the changelog](../CHANGELOG.md) before changing versions.

## Develop and release

- [Development](development.md): source layout, prerequisites, local checks, and
  release archive contents.
- [Release maintenance](releasing.md): versioning, SDK pinning, native build gates,
  and publication.
- [AGENTS.md](https://github.com/oai404iao/ai-gateway-connectors/blob/main/AGENTS.md):
  repository guidance for coding agents.
