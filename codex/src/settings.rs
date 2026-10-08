//! Provider-owned configuration, validated before an immutable generation is published.

use ai_gateway_connector_sdk::{PluginCallError, PluginOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub workspace_path: String,
    pub git_remote_url: String,
    pub originator: String,
    pub client_version: String,
    pub user_agent: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace_path: "/workspace".into(),
            git_remote_url: "https://github.com/oai404iao/ai_gateway".into(),
            originator: "codex_cli_rs".into(),
            client_version: "0.146.0".into(),
            user_agent: "codex_cli_rs/0.146.0".into(),
        }
    }
}

impl Settings {
    pub fn from_metadata(metadata: &Value) -> Result<Self, PluginCallError> {
        serde_json::from_value(metadata["settings"].clone())
            .map_err(|_| PluginCallError::new("invalid_settings", "Missing compiled settings"))
    }
}

fn valid_header(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= maximum
        && http::HeaderValue::from_str(value).is_ok()
}

fn validate(values: &Value) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(object) = values.as_object() else {
        return vec![json!({"field":"workspace_path","code":"invalid_object"})];
    };
    let fields = [
        "workspace_path",
        "git_remote_url",
        "originator",
        "client_version",
        "user_agent",
    ];
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        errors.push(json!({"field":"workspace_path","code":"unknown_field"}));
    }
    for (key, maximum) in [
        ("originator", 256),
        ("client_version", 128),
        ("user_agent", 1024),
    ] {
        if !object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|v| valid_header(v, maximum))
        {
            errors.push(json!({"field":key,"code":"invalid_header"}));
        }
    }
    if !object
        .get("workspace_path")
        .and_then(Value::as_str)
        .is_some_and(|v| {
            v.starts_with('/')
                && v.trim() == v
                && v.chars().count() <= 1024
                && !v.chars().any(char::is_control)
        })
    {
        errors.push(json!({"field":"workspace_path","code":"invalid_absolute_path"}));
    }
    if !object
        .get("git_remote_url")
        .and_then(Value::as_str)
        .is_some_and(|v| {
            v.trim() == v
                && v.chars().count() <= 2048
                && url::Url::parse(v).is_ok_and(|url| {
                    url.scheme() == "https"
                        && url.host_str().is_some()
                        && url.path() != "/"
                        && url.username().is_empty()
                        && url.password().is_none()
                        && url.query().is_none()
                        && url.fragment().is_none()
                })
        })
    {
        errors.push(json!({"field":"git_remote_url","code":"invalid_git_url"}));
    }
    errors
}

pub(crate) fn dispatch(command: &str, metadata: Value) -> Result<PluginOutput, PluginCallError> {
    let output = match command {
        "settings.describe/v1" => {
            let fields = [
                (
                    "workspace_path",
                    "合成工作区路径",
                    "Synthetic workspace path",
                    1024,
                ),
                (
                    "git_remote_url",
                    "合成 Git 地址",
                    "Synthetic Git origin",
                    2048,
                ),
                ("originator", "来源标识", "Originator", 256),
                ("client_version", "客户端版本", "Client version", 128),
                ("user_agent", "User-Agent", "User-Agent", 1024),
            ]
            .map(|(key, zh, en, maximum)| {
                json!({
                    "key":key,"label":{"zh-CN":zh,"en":en},
                    "required":true,"type":"string","max_length":maximum
                })
            });
            json!({"schema_version":1,"title":{"zh-CN":"Codex 设置","en":"Codex settings"},
                "fields":fields,"defaults":Settings::default()})
        }
        "settings.validate/v1" | "settings.compile/v1" => {
            let mut errors = validate(&metadata["values"]);
            if metadata["schema_version"].as_u64() != Some(1) {
                errors.push(json!({"field":"workspace_path","code":"unsupported_schema"}));
            }
            if command == "settings.validate/v1" {
                json!({"valid":errors.is_empty(),"errors":errors})
            } else {
                if !errors.is_empty() {
                    return Err(PluginCallError::new("invalid_settings", "Invalid settings"));
                }
                json!({"config":metadata["values"]})
            }
        }
        "settings.migrate/v1" => {
            if metadata["from_schema_version"].as_u64() != Some(1)
                || !validate(&metadata["values"]).is_empty()
            {
                return Err(PluginCallError::new(
                    "unsupported_schema",
                    "Unsupported settings migration",
                ));
            }
            json!({"schema_version":1,"values":metadata["values"]})
        }
        _ => {
            return Err(PluginCallError::new(
                "unsupported_command",
                "Unsupported settings command",
            ));
        }
    };
    Ok(PluginOutput {
        metadata: output,
        body: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_matches_the_generic_sdk_contract() {
        let output = dispatch("settings.describe/v1", json!({})).unwrap();
        let descriptor: ai_gateway_connector_sdk::PluginSettingsDescriptor =
            serde_json::from_value(output.metadata).unwrap();
        assert!(descriptor.validate_descriptor());
        assert_eq!(descriptor.fields.len(), 5);
        assert!(descriptor.fields.iter().all(|field| field.required));
    }

    #[test]
    fn defaults_and_custom_identity_compile_without_host_interpretation() {
        let defaults = serde_json::to_value(Settings::default()).unwrap();
        assert!(validate(&defaults).is_empty());
        let mut values = defaults.clone();
        values["client_version"] = json!("9.8.7");
        let out = dispatch(
            "settings.compile/v1",
            json!({"schema_version":1,"values":values}),
        )
        .unwrap();
        assert_eq!(out.metadata["config"]["client_version"], "9.8.7");
        assert_eq!(defaults["client_version"], "0.146.0");
        assert!(Settings::from_metadata(&json!({})).is_err());
    }

    #[test]
    fn invalid_and_unknown_fields_fail_closed() {
        for (key, value) in [
            ("workspace_path", "relative"),
            ("git_remote_url", "https://user:pass@example.com/project"),
            ("git_remote_url", "https://example.com/project?secret=value"),
            ("client_version", "bad\r\nheader"),
            ("originator", ""),
            ("unexpected", "value"),
        ] {
            let mut values = serde_json::to_value(Settings::default()).unwrap();
            values[key] = json!(value);
            assert!(!validate(&values).is_empty(), "{key}");
            assert!(
                dispatch(
                    "settings.compile/v1",
                    json!({"schema_version":1,"values":values})
                )
                .is_err()
            );
        }
        assert!(
            dispatch(
                "settings.migrate/v1",
                json!({"from_schema_version":2,"values":Settings::default()})
            )
            .is_err()
        );
    }
}
