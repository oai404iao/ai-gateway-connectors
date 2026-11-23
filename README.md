# ai-gateway-connectors

The Codex plugin lets [ai-gateway](https://github.com/oai404iao/ai_gateway) use ChatGPT subscriptions for Responses HTTP and WebSocket, standalone search, and image generation and editing. It prepares provider requests and interprets account data. The gateway makes the network calls, stores credentials, routes requests and accounts for usage.

## Get started

1. [Choose and install a trusted native package](docs/deployment.md) for your gateway and Linux architecture. The plugin runs with the gateway process's privileges.
2. Check the [provider identity and synthetic workspace settings](docs/configuration.md), then enable the artifact in gateway administration.
3. [Connect an account and assign routes](https://github.com/oai404iao/ai_gateway/blob/main/docs/user/codex.md). Installing the plugin does not create a credential or authorize clients.

See the [Codex reference](docs/codex.md) for the operation and transport contract. Contributors can [build and test](docs/development.md) or [prepare a release](docs/releasing.md).

## License

Licensed under [AGPL-3.0-only](LICENSE). Redistribute binaries with their corresponding source obligations, notices and dependency licenses. Release packages include those notices and identify the source commit and SDK dependency.
