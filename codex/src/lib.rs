//! Codex provider protocol implementation for the gateway connector ABI.

mod attempt;
mod protocol;

use ai_gateway_connector_sdk::{PluginCallError, PluginManifest, PluginOutput};
use serde_json::Value;

pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "codex".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        operations: [
            "responses",
            "responses-ws",
            "web_search",
            "images_generation",
            "images_edit",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        commands: [
            "endpoints",
            "authorize_url",
            "parse_callback",
            "parse_identity",
            "parse_expiration",
            "exchange_plan",
            "exchange_parse",
            "refresh_plan",
            "refresh_parse",
            "models_plan",
            "models_parse",
            "quota_plan",
            "quota_parse",
            "quota_reset_plan",
            "quota_reset_parse",
            "attempt.body",
            "attempt.target",
            "attempt.headers",
            "attempt.capabilities",
            "attempt.image_edit_plan",
            "attempt.image_part_plan",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

fn dispatch(command: &str, metadata: Value, body: &[u8]) -> Result<PluginOutput, PluginCallError> {
    if command.starts_with("attempt.") {
        attempt::dispatch(command, metadata, body)
    } else {
        protocol::dispatch(command, metadata, body)
    }
}

ai_gateway_connector_sdk::export_plugin!(manifest, dispatch);
