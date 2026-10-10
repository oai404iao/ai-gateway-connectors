# Changelog

All notable changes follow Keep a Changelog and Semantic Versioning.

## [Unreleased]

### Changed

- Codex declares protocol 3 with explicit operation transports and pass-through
  responses, preserving ABI 1, settings schema 1 and the reviewed SDK pin.
- Usage descriptors reuse the gateway's general parser for the actual upstream
  Responses counter interface across HTTP, WebSocket, Search and Images.
- Native package verification validates pure typed descriptors and rejects
  response adaptation; packaging supports isolated `CARGO_TARGET_DIR` builds.

## [0.2.0] - 2026-10-09

### Added

- Plugin-owned settings descriptions, defaults, validation, compilation, and
  explicit schema migration commands for generic gateway management.
- Credential-stable installation identity and provider-owned request privacy
  normalization, outbound body/Header policy, and multipart field validation.
- Tests for settings isolation, fingerprint removal, legacy installation UUID
  compatibility, and preservation of filtered billing metadata.

### Changed

- Requires manifest protocol version 2 and SDK 0.2's immutable configured-generation
  contract; native C ABI remains version 1. Older hosts reject the manifest.
- Request and maintenance commands consume compiled plugin settings instead of
  gateway-interpreted identity fields. Request identity is opaque to the gateway.
- Installation and deployment guidance uses managed directories and explicit
  hot activation, with generation-isolated requests and retained native libraries.

## [0.1.0] - 2026-10-08

### Added

- Independently installed Codex connector implementing ai-gateway native ABI v1.
- OAuth exchange/refresh, model discovery, quota observations, and Responses,
  WebSocket, standalone search, and Images request adaptation commands.
- Linux x86_64/aarch64 GNU release archives with manifests, SHA-256 checksums,
  source/SDK provenance, AGPL license, and dependency notices.
