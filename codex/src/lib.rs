//! Codex provider protocol implementation for the gateway connector ABI.

mod attempt;
mod identity;
mod protocol;
mod request_policy;
mod settings;

use ai_gateway_connector_sdk::{PluginCallError, PluginManifest, PluginOutput};
use serde_json::Value;

pub fn manifest() -> PluginManifest {
    PluginManifest {
        id: "codex".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        protocol_version: 2,
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
            "settings.describe/v1",
            "settings.validate/v1",
            "settings.compile/v1",
            "settings.migrate/v1",
            "attempt.context",
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
    if command.starts_with("settings.") {
        settings::dispatch(command, metadata)
    } else if command.starts_with("attempt.") {
        attempt::dispatch(command, metadata, body)
    } else {
        protocol::dispatch(command, metadata, body)
    }
}

ai_gateway_connector_sdk::export_plugin!(manifest, dispatch);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn configured(mut metadata: Value) -> Value {
        let mut settings = serde_json::to_value(settings::Settings::default()).unwrap();
        settings["workspace_path"] = json!("/synthetic/project");
        settings["git_remote_url"] = json!("https://example.test/synthetic");
        settings["client_version"] = json!("9.8.7");
        settings["originator"] = json!("configured-codex");
        metadata["settings"] = dispatch(
            "settings.compile/v1",
            json!({"schema_version":1,"values":settings}),
            &[],
        )
        .unwrap()
        .metadata["config"]
            .clone();
        metadata
    }

    #[test]
    fn abi_dispatch_owns_privacy_and_never_restores_filtered_fast_metadata() {
        let context = dispatch(
            "attempt.context",
            configured(json!({
                "operation":"responses",
                "credential_id":"11111111-1111-4111-8111-111111111111",
                "request_id":"22222222-2222-4222-8222-222222222222",
                "headers":{"session-id":"session","thread-id":"thread"}
            })),
            &[],
        )
        .unwrap()
        .metadata;
        let raw_turn = json!({"installation_id":"client-fingerprint","workspaces":{
            "/home/private/project":{"associated_remote_urls":{"origin":"https://private.test/repo"},
                "commit":"private-commit","dirty":true}
        }})
        .to_string();
        let body = json!({"model":"selected","input":[],"stream":true,"store":true,
            "client_metadata":{"x-codex-installation-id":"client-fingerprint","x-codex-turn-metadata":raw_turn}});
        let adapted = dispatch(
            "attempt.body",
            configured(json!({"operation":"responses","protocol":"sse","request_context":context})),
            &serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
        let adapted: Value = serde_json::from_slice(&adapted.body).unwrap();
        assert!(adapted.get("service_tier").is_none());
        assert_eq!(adapted["model"], "selected");
        assert_eq!(adapted["store"], false);
        assert_eq!(
            adapted["client_metadata"]["x-codex-installation-id"],
            "d3c8659c-e5a3-42ed-aa68-abd014393d91"
        );
        let turn: Value = serde_json::from_str(
            adapted["client_metadata"]["x-codex-turn-metadata"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            turn["workspaces"]["/synthetic/project"]["associated_remote_urls"]["origin"],
            "https://example.test/synthetic"
        );
        for private in [
            "client-fingerprint",
            "/home/private",
            "private-commit",
            "private.test",
        ] {
            assert!(!adapted.to_string().contains(private));
        }
        for operation in [
            "responses",
            "responses-ws",
            "web_search",
            "images_generation",
            "images_edit",
        ] {
            let result = dispatch(
                "attempt.headers",
                configured(json!({
                    "operation":operation,
                    "protocol":if operation=="responses-ws" {"websocket"} else {"non_stream"},
                    "request_context":context,"access_token":"selected-token",
                    "headers":{"x-codex-turn-metadata":raw_turn,"x-private-transform":"discard"}
                })),
                &[],
            )
            .unwrap();
            assert_eq!(result.metadata["set"]["version"], "9.8.7");
            assert_eq!(result.metadata["set"]["originator"], "configured-codex");
            assert!(
                result.metadata["remove"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("x-private-transform"))
            );
            let turn: Value = serde_json::from_str(
                result.metadata["set"]["x-codex-turn-metadata"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                turn["installation_id"],
                "d3c8659c-e5a3-42ed-aa68-abd014393d91"
            );
            assert!(turn["workspaces"].get("/home/private/project").is_none());
            if operation == "responses-ws" {
                assert!(turn.get("turn_id").is_none());
            }
        }
    }

    #[test]
    fn maintenance_plans_use_only_compiled_plugin_identity() {
        let result = dispatch(
            "models_plan",
            configured(json!({
                "endpoints":{"issuer":"https://auth.openai.com",
                    "responses_base_url":"https://chatgpt.com/backend-api/codex"},
                "access_token":"selected-token","account_id":null,"is_fedramp":false,
                "identity":{"originator":"must-not-be-used","client_version":"old","user_agent":"old"}
            })),
            &[],
        )
        .unwrap();
        let plan = &result.metadata["result"];
        assert_eq!(plan["headers"]["version"], "9.8.7");
        assert_eq!(plan["headers"]["originator"], "configured-codex");
        assert!(
            plan["url"]
                .as_str()
                .unwrap()
                .contains("client_version=9.8.7")
        );
        let rejected = dispatch(
            "models_plan",
            json!({"identity":{"originator":"legacy"}}),
            &[],
        )
        .unwrap();
        assert!(rejected.metadata.get("protocol_error").is_some());
        assert!(rejected.metadata.get("result").is_none());
    }
}
