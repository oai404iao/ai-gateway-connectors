//! Stateless data-plane adaptation; the host owns credentials and byte storage.

use ai_gateway_connector_sdk::{PluginCallError, PluginOutput};
use serde_json::{Map, Value, json};

fn error(code: &str) -> PluginCallError {
    PluginCallError::new(code, "Codex request adaptation failed")
}

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, PluginCallError> {
    value[name]
        .as_str()
        .ok_or_else(|| error("invalid_request_body"))
}

pub fn dispatch(command: &str, m: Value, body: &[u8]) -> Result<PluginOutput, PluginCallError> {
    let mut output = PluginOutput {
        metadata: json!({}),
        body: Vec::new(),
    };
    match command {
        "attempt.body" => {
            let protocol = string(&m, "protocol")?;
            match string(&m, "operation")? {
                "responses" | "responses-ws" if protocol == "non_stream" => {
                    return Err(error("streaming_required"));
                }
                "images_generation" if protocol != "non_stream" => {
                    return Err(error("image_streaming_unsupported"));
                }
                "web_search" if protocol != "non_stream" => {
                    return Err(error("search_streaming_unsupported"));
                }
                "images_edit" => return Err(error("invalid_request_body")),
                "responses" | "responses-ws" | "images_generation" | "web_search" => {}
                _ => return Err(error("unsupported_operation")),
            }
            output.body = body.to_vec();
        }
        "attempt.target" => {
            let path = match string(&m, "operation")? {
                "responses" | "responses-ws" => "responses",
                "web_search" => "alpha/search",
                "images_generation" => "images/generations",
                "images_edit" => "images/edits",
                _ => return Err(error("unsupported_operation")),
            };
            let base = string(&m, "base_url")?.trim_end_matches('/');
            let query = m["query"]
                .as_str()
                .map(|q| format!("?{q}"))
                .unwrap_or_default();
            let url = url::Url::parse(&format!("{base}/{path}{query}"))
                .map_err(|_| error("invalid_target"))?;
            output.metadata = json!({"url": url.as_str()});
        }
        "attempt.headers" => output.metadata = headers(&m)?,
        "attempt.capabilities" => {
            let operation = string(&m, "operation")?;
            let responses = matches!(operation, "responses" | "responses-ws");
            output.metadata = json!({
                "preserves_affinity_on_failure": responses || operation == "web_search",
                "successful_response_is_sse": responses,
                "changes_request_body": responses || matches!(operation, "images_generation" | "images_edit")
            });
        }
        "attempt.image_edit_plan" => {
            let mut input = m;
            input["fields"] = serde_json::from_slice(body).map_err(|_| error("invalid_field"))?;
            let plan = image_edit_plan(&input)?;
            let prefix = string(&plan, "prefix")?;
            let suffix = string(&plan, "suffix")?;
            output.metadata = json!({"body_mode":"json_base64","prefix_bytes":prefix.len()});
            output.body = [prefix.as_bytes(), suffix.as_bytes()].concat();
        }
        "attempt.image_part_plan" => {
            let mime = if let Some(mime) = m["content_type"].as_str() {
                if !mime.starts_with("image/") || mime.len() > 128 {
                    return Err(error("image_content_type"));
                }
                mime.to_owned()
            } else {
                let name = string(&m, "file_name")
                    .map_err(|_| error("image_content_type"))?
                    .to_ascii_lowercase();
                if name.ends_with(".png") {
                    "image/png"
                } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
                    "image/jpeg"
                } else if name.ends_with(".webp") {
                    "image/webp"
                } else if name.ends_with(".gif") {
                    "image/gif"
                } else {
                    return Err(error("image_content_type"));
                }
                .to_owned()
            };
            let encoded = serde_json::to_string(&format!("data:{mime};base64,"))
                .map_err(|_| error("invalid_request_body"))?;
            let comma = if m["index"].as_u64().unwrap_or(0) == 0 {
                ""
            } else {
                ","
            };
            output.metadata = json!({"prefix":format!("{comma}{{\"image_url\":{}", &encoded[..encoded.len()-1]), "suffix":"\"}"});
        }
        _ => return Err(error("unsupported_command")),
    }
    Ok(output)
}

fn headers(m: &Value) -> Result<Value, PluginCallError> {
    let operation = string(m, "operation")?;
    let mut set = Map::new();
    let mut remove = Vec::new();
    for (header, key) in [
        ("user-agent", "user_agent"),
        ("originator", "originator"),
        ("version", "client_version"),
        ("session-id", "session_id"),
        ("thread-id", "thread_id"),
    ] {
        set.insert(header.into(), Value::String(string(m, key)?.into()));
    }
    set.insert(
        "authorization".into(),
        Value::String(format!("Bearer {}", string(m, "access_token")?)),
    );
    if let Some(account) = m["account_id"].as_str() {
        set.insert("chatgpt-account-id".into(), json!(account));
    } else {
        remove.push("chatgpt-account-id");
    }
    if m["is_fedramp"].as_bool() == Some(true) {
        set.insert("x-openai-fedramp".into(), json!("true"));
    } else {
        remove.push("x-openai-fedramp");
    }
    if m["headers"]["x-client-request-id"].is_null() {
        set.insert("x-client-request-id".into(), m["thread_id"].clone());
    }
    match operation {
        "responses" | "responses-ws" => {
            if m["protocol"] == "websocket" {
                remove.extend([
                    "accept",
                    "accept-encoding",
                    "content-encoding",
                    "content-type",
                ]);
            } else {
                set.insert("accept".into(), json!("text/event-stream"));
                set.insert("content-type".into(), json!("application/json"));
            }
            remove.push("x-codex-image-turn-id");
        }
        "images_generation" | "images_edit" => {
            set.insert("accept".into(), json!("application/json"));
            set.insert("content-type".into(), json!("application/json"));
            let turn = m["headers"]["x-codex-image-turn-id"]
                .as_str()
                .map(str::trim)
                .filter(|v| !v.is_empty() && v.len() <= 512)
                .unwrap_or(string(m, "turn_id")?);
            set.insert("x-codex-image-turn-id".into(), json!(turn));
        }
        "web_search" => {
            set.insert("accept".into(), json!("application/json"));
            set.insert("content-type".into(), json!("application/json"));
            remove.push("x-codex-image-turn-id");
        }
        _ => return Err(error("unsupported_operation")),
    }
    for (name, value) in &set {
        http::HeaderName::from_bytes(name.as_bytes()).map_err(|_| error("invalid_credentials"))?;
        http::HeaderValue::from_str(value.as_str().ok_or_else(|| error("invalid_credentials"))?)
            .map_err(|_| error("invalid_credentials"))?;
    }
    Ok(json!({"set":set, "remove":remove}))
}

fn image_edit_plan(m: &Value) -> Result<Value, PluginCallError> {
    if m["image_count"].as_u64().unwrap_or(0) > 5 {
        return Err(error("too_many_images"));
    }
    if m["mask_count"].as_u64().unwrap_or(0) > 0 {
        return Err(error("mask_unsupported"));
    }
    let fields = m["fields"]
        .as_array()
        .ok_or_else(|| error("invalid_field"))?;
    let get = |name: &str| -> Result<Option<&str>, PluginCallError> {
        let mut matching = fields.iter().filter(|f| f["name"] == name);
        let first = matching.next();
        if matching.next().is_some() {
            return Err(error("duplicate_field"));
        }
        first
            .map(|v| string(v, "value").map_err(|_| error("invalid_field")))
            .transpose()
    };
    let mut object = Map::new();
    for required in ["prompt", "model"] {
        object.insert(
            required.into(),
            json!(get(required)?.ok_or_else(|| error("missing_field"))?),
        );
    }
    for (name, allowed) in [
        ("background", &["transparent", "opaque", "auto"][..]),
        ("quality", &["low", "medium", "high", "auto"][..]),
        ("size", &[][..]),
    ] {
        if let Some(value) = get(name)? {
            if !allowed.is_empty() && !allowed.contains(&value) {
                return Err(error("invalid_field"));
            }
            object.insert(name.into(), json!(value));
        }
    }
    if let Some(n) = get("n")? {
        object.insert(
            "n".into(),
            json!(
                n.trim()
                    .parse::<u64>()
                    .map_err(|_| error("invalid_field"))?
            ),
        );
    }
    if let Some(stream) = get("stream")?
        && !(stream.trim().eq_ignore_ascii_case("false") || stream.trim() == "0")
    {
        return Err(error("image_streaming_unsupported"));
    }
    let fields = serde_json::to_string(&object).map_err(|_| error("invalid_field"))?;
    Ok(json!({"prefix":"{\"images\":[", "suffix":format!("],{}", &fields[1..])}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_context(operation: &str, protocol: &str) -> Value {
        json!({
            "operation":operation,"protocol":protocol,
            "access_token":"access-token","account_id":"account-123","is_fedramp":false,
            "user_agent":"codex_gateway/9.8.7","originator":"codex_gateway","client_version":"9.8.7",
            "session_id":"session","thread_id":"thread","turn_id":"generated-turn","headers":{}
        })
    }

    #[test]
    fn header_plans_preserve_provider_auth_identity_and_transport_rules() {
        for operation in [
            "responses",
            "responses-ws",
            "web_search",
            "images_generation",
            "images_edit",
        ] {
            let mut metadata = header_context(operation, "sse");
            let result = headers(&metadata).unwrap();
            assert_eq!(result["set"]["authorization"], "Bearer access-token");
            assert_eq!(result["set"]["chatgpt-account-id"], "account-123");
            assert_eq!(result["set"]["originator"], "codex_gateway");
            assert_eq!(result["set"]["version"], "9.8.7");
            assert_eq!(result["set"]["user-agent"], "codex_gateway/9.8.7");
            assert_eq!(result["set"]["session-id"], "session");
            assert_eq!(result["set"]["thread-id"], "thread");
            assert_eq!(result["set"]["x-client-request-id"], "thread");
            metadata["headers"]["x-client-request-id"] = json!("caller-request");
            metadata["headers"]["x-codex-image-turn-id"] = json!("caller-turn");
            metadata["is_fedramp"] = json!(true);
            metadata["account_id"] = Value::Null;
            let result = headers(&metadata).unwrap();
            assert!(result["set"].get("x-client-request-id").is_none());
            assert_eq!(result["set"]["x-openai-fedramp"], "true");
            assert!(
                result["remove"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("chatgpt-account-id"))
            );
            if operation.starts_with("images_") {
                assert_eq!(result["set"]["x-codex-image-turn-id"], "caller-turn");
                assert_eq!(result["set"]["accept"], "application/json");
            } else {
                assert!(
                    result["remove"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("x-codex-image-turn-id"))
                );
            }
        }
        let result = headers(&header_context("responses-ws", "websocket")).unwrap();
        for header in [
            "accept",
            "accept-encoding",
            "content-encoding",
            "content-type",
        ] {
            assert!(
                result["remove"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(header))
            );
            assert!(result["set"].get(header).is_none());
        }
        for header in [
            "x-codex-beta-features",
            "x-codex-routing-hint",
            "x-codex-turn-state",
            "x-openai-internal-codex-responses-lite",
        ] {
            assert!(result["set"].get(header).is_none());
        }
        let mut invalid = header_context("responses", "sse");
        invalid["access_token"] = json!("token\r\ninjected:true");
        assert!(headers(&invalid).is_err());
    }

    #[test]
    fn image_turn_defaults_only_when_caller_has_no_valid_turn() {
        for turn in [Value::Null, json!(" "), json!("x".repeat(513))] {
            let mut context = header_context("images_generation", "non_stream");
            context["headers"]["x-codex-image-turn-id"] = turn;
            assert_eq!(
                headers(&context).unwrap()["set"]["x-codex-image-turn-id"],
                "generated-turn"
            );
        }
    }

    #[test]
    fn targets_and_protocol_requirements_are_provider_owned() {
        for (operation, path) in [
            ("responses", "responses"),
            ("web_search", "alpha/search"),
            ("images_generation", "images/generations"),
            ("images_edit", "images/edits"),
        ] {
            let out = dispatch("attempt.target", json!({"operation":operation,"base_url":"https://example.test/backend-api/codex/","query":"x=1"}), &[]).unwrap();
            assert_eq!(
                out.metadata["url"],
                format!("https://example.test/backend-api/codex/{path}?x=1")
            );
        }
        assert!(
            dispatch(
                "attempt.body",
                json!({"operation":"responses","protocol":"non_stream"}),
                b"{}"
            )
            .is_err()
        );
        let raw = br#"{ "stream":true }"#;
        assert_eq!(
            dispatch(
                "attempt.body",
                json!({"operation":"responses","protocol":"sse"}),
                raw
            )
            .unwrap()
            .body,
            raw
        );
    }

    #[test]
    fn image_plan_streams_literals_around_host_base64() {
        let m = json!({"image_count":1,"mask_count":0,"fields":[{"name":"model","value":"gpt-image"},{"name":"prompt","value":"a\n\"b"},{"name":"n","value":"2"}]});
        let plan = image_edit_plan(&m).unwrap();
        let part = dispatch(
            "attempt.image_part_plan",
            json!({"content_type":"image/png","index":0}),
            &[],
        )
        .unwrap()
        .metadata;
        let result = format!(
            "{}{}aGVsbG8={}{}",
            plan["prefix"].as_str().unwrap(),
            part["prefix"].as_str().unwrap(),
            part["suffix"].as_str().unwrap(),
            plan["suffix"].as_str().unwrap()
        );
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["images"][0]["image_url"],
            "data:image/png;base64,aGVsbG8="
        );
        assert_eq!(parsed["prompt"], "a\n\"b");
        assert_eq!(parsed["n"], 2);
        let mut bad = m.clone();
        bad["image_count"] = json!(6);
        assert!(image_edit_plan(&bad).is_err());
        bad = m.clone();
        bad["mask_count"] = json!(1);
        assert!(image_edit_plan(&bad).is_err());
        bad = m;
        bad["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"prompt","value":"duplicate"}));
        assert!(image_edit_plan(&bad).is_err());
    }

    #[test]
    fn image_plan_validation_and_raw_template_contract() {
        let fields = json!([
            {"name":"model","value":"gpt-image"}, {"name":"prompt","value":"escaped\n\"text"},
            {"name":"background","value":"auto"}, {"name":"quality","value":"high"},
            {"name":"size","value":"1024x1024"}, {"name":"stream","value":"false"}
        ]);
        let context = json!({"image_count":1,"mask_count":0});
        let output = dispatch(
            "attempt.image_edit_plan",
            context.clone(),
            &serde_json::to_vec(&fields).unwrap(),
        )
        .unwrap();
        let prefix = output.metadata["prefix_bytes"].as_u64().unwrap() as usize;
        assert_eq!(output.metadata["body_mode"], "json_base64");
        assert_eq!(&output.body[..prefix], br#"{"images":["#);
        let skeleton = serde_json::from_slice::<Value>(&output.body).unwrap();
        assert_eq!(skeleton["model"], "gpt-image");
        assert_eq!(skeleton["prompt"], "escaped\n\"text");
        assert!(skeleton.get("stream").is_none());
        for (name, value) in [
            ("n", "wrong"),
            ("stream", "true"),
            ("quality", "invalid"),
            ("background", "invalid"),
        ] {
            let mut bad = fields
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["name"] != name)
                .cloned()
                .collect::<Vec<_>>();
            bad.push(json!({"name":name,"value":value}));
            assert!(
                dispatch(
                    "attempt.image_edit_plan",
                    context.clone(),
                    &serde_json::to_vec(&bad).unwrap()
                )
                .is_err()
            );
        }
        for (name, mime) in [
            ("a.png", "image/png"),
            ("a.JPG", "image/jpeg"),
            ("a.webp", "image/webp"),
            ("a.gif", "image/gif"),
        ] {
            let output = dispatch(
                "attempt.image_part_plan",
                json!({"file_name":name,"index":1}),
                &[],
            )
            .unwrap();
            assert!(output.metadata["prefix"].as_str().unwrap().starts_with(","));
            assert!(output.metadata["prefix"].as_str().unwrap().contains(mime));
        }
        for m in [
            json!({"file_name":"unknown.bin"}),
            json!({"content_type":"text/plain"}),
        ] {
            assert!(dispatch("attempt.image_part_plan", m, &[]).is_err());
        }
    }
}
