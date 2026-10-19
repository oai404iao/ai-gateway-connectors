# Changelog

Notable changes are recorded here using Keep a Changelog and Semantic Versioning.
For the current source contract, see [Codex compatibility](docs/codex.md).

## [Unreleased]

### Changed

- Codex requires command protocol 3, with explicit operation transports and
  pass-through responses. ABI 1, settings schema 1, and the reviewed SDK pin are
  unchanged.
- Usage descriptors select the gateway's general parser for upstream Responses
  counters across HTTP, WebSocket, standalone search, and Images.
- Native package verification checks pure typed descriptors and rejects response
  adaptation. Packaging supports isolated `CARGO_TARGET_DIR` builds.
- User documentation is organized into installation, configuration, Codex
  reference, development, and release guides, with a documentation index and
  repository `AGENTS.md`.

## [0.2.0] - 2026-10-09

### Added

- Plugin-owned settings descriptions, defaults, validation, compilation, and
  explicit schema migration commands for gateway management.
- Credential-stable installation identity, provider-owned privacy normalization,
  outbound body/header policy, and multipart field validation.
- Tests for settings isolation, fingerprint removal, pre-split installation UUID
  compatibility, and preservation of gateway billing-metadata filtering.

### Changed

- Required manifest protocol 2 and SDK 0.2's immutable configured-generation
  contract; native C ABI remained version 1. Older hosts reject the manifest.
- Request and maintenance commands use compiled plugin settings instead of
  gateway-interpreted identity fields. Request identity is opaque to the host.
- Installation guidance uses managed directories and explicit hot activation,
  with generation-isolated requests and retained native libraries.

## [0.1.0] - 2026-10-08

### Added

- Independently installed Codex connector implementing ai-gateway native ABI 1.
- OAuth exchange/refresh, model discovery, quota observations, and Responses,
  WebSocket, standalone search, and Images request adaptation commands.
- GNU Linux x86_64/aarch64 release archives with manifests, SHA-256 checksums,
  source and SDK provenance, AGPL license, and dependency notices.
