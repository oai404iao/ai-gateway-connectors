//! Stateless data-plane adaptation; the host owns credentials and byte storage.

use crate::{
    identity::RequestIdentity,
    request_policy::{self, RequestInterface},
    settings::Settings,
};
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

impl RequestInterface {
    fn for_operation(m: &Value) -> Result<Self, PluginCallError> {
        match string(m, "operation")? {
            "responses" => Ok(Self::ResponsesHttp),
            "responses-ws" => Ok(Self::ResponsesWebSocket),
            "web_search" => Ok(Self::StandaloneWebSearch),
            "images_generation" => Ok(Self::ImagesGeneration),
            "images_edit" => Ok(Self::ImagesEdit),
            _ => Err(error("unsupported_operation")),
        }
    }

    fn protocol(self) -> &'static str {
        match self {
            Self::ResponsesHttp => "sse",
            Self::ResponsesWebSocket => "websocket",
            Self::StandaloneWebSearch | Self::ImagesGeneration | Self::ImagesEdit => "non_stream",
        }
    }

    fn for_request(m: &Value) -> Result<Self, PluginCallError> {
        let protocol = string(m, "protocol")?;
        let interface = Self::for_operation(m)?;
        if protocol != interface.protocol() {
            return Err(error(match interface {
                Self::ResponsesHttp | Self::ResponsesWebSocket if protocol == "non_stream" => {
                    "streaming_required"
                }
                Self::StandaloneWebSearch => "search_streaming_unsupported",
                Self::ImagesGeneration => "image_streaming_unsupported",
                _ => "invalid_request_body",
            }));
        }
        Ok(interface)
    }

    fn capabilities(self) -> Value {
        let responses = matches!(self, Self::ResponsesHttp | Self::ResponsesWebSocket);
        json!({
            "preserves_affinity_on_failure": responses || self == Self::StandaloneWebSearch,
            "successful_response_is_sse": responses,
            "changes_request_body": self != Self::StandaloneWebSearch
        })
    }
}

pub fn dispatch(command: &str, m: Value, body: &[u8]) -> Result<PluginOutput, PluginCallError> {
    let mut output = PluginOutput {
        metadata: json!({}),
        body: Vec::new(),
    };
    match command {
        "attempt.context" => {
            output.metadata = serde_json::to_value(RequestIdentity::prepare(&m)?)
                .map_err(|_| error("invalid_request_context"))?
        }
        "attempt.body" => {
            let interface = RequestInterface::for_request(&m)?;
            if interface == RequestInterface::ImagesEdit {
                return Err(error("invalid_request_body"));
            }
            let identity = RequestIdentity::from_metadata(&m)?;
            let settings = Settings::from_metadata(&m)?;
            let filtered = request_policy::apply_json_body_policy(
                interface,
                bytes::Bytes::copy_from_slice(body),
            )?;
            output.body = request_policy::normalize_codex_fingerprints_in_json(
                interface,
                filtered.body,
                &identity.privacy(&settings),
            )?
            .body
            .to_vec();
        }
        "attempt.target" => {
            let path = match RequestInterface::for_operation(&m)? {
                RequestInterface::ResponsesHttp | RequestInterface::ResponsesWebSocket => {
                    "responses"
                }
                RequestInterface::StandaloneWebSearch => "alpha/search",
                RequestInterface::ImagesGeneration => "images/generations",
                RequestInterface::ImagesEdit => "images/edits",
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
            output.metadata = RequestInterface::for_operation(&m)?.capabilities();
        }
        "attempt.describe/v1" => {
            if !body.is_empty() {
                return Err(error("invalid_request_body"));
            }
            let interface = RequestInterface::for_operation(&m)?;
            output.metadata = json!({
                "capabilities": interface.capabilities(),
                "protocols":[{"protocol":interface.protocol(),"response":"passthrough"}],
                "usage":{"parser":"general","format":"open_ai_responses"}
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
    let identity = RequestIdentity::from_metadata(m)?;
    let settings = Settings::from_metadata(m)?;
    let interface = RequestInterface::for_request(m)?;
    let mut original = http::HeaderMap::new();
    for (name, value) in m["headers"]
        .as_object()
        .ok_or_else(|| error("invalid_request_context"))?
    {
        if value.is_null() {
            continue;
        }
        let name = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| error("invalid_request_context"))?;
        let value = http::HeaderValue::from_str(
            value
                .as_str()
                .ok_or_else(|| error("invalid_request_context"))?,
        )
        .map_err(|_| error("invalid_request_context"))?;
        original.insert(name, value);
    }
    let mut filtered = request_policy::filter_codex_headers(interface, &original)?;
    request_policy::normalize_codex_fingerprints_in_headers(
        interface,
        &mut filtered,
        &identity.privacy(&settings),
    );
    let mut context = m.clone();
    context["headers"] = json!(
        filtered
            .iter()
            .map(|(n, v)| (n.as_str(), v.to_str().unwrap_or("")))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    let m = &context;
    let mut set = Map::new();
    let mut remove = original
        .keys()
        .filter(|name| !filtered.contains_key(*name))
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    for (name, value) in &filtered {
        if original.get(name) != Some(value) {
            set.insert(
                name.to_string(),
                json!(
                    value
                        .to_str()
                        .map_err(|_| error("invalid_request_context"))?
                ),
            );
        }
    }
    for (header, value) in [
        ("user-agent", settings.user_agent.as_str()),
        ("originator", settings.originator.as_str()),
        ("version", settings.client_version.as_str()),
        ("session-id", identity.session_id.as_str()),
        ("thread-id", identity.thread_id.as_str()),
    ] {
        set.insert(header.into(), Value::String(value.into()));
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
        set.insert("x-client-request-id".into(), json!(identity.thread_id));
    }
    match interface {
        RequestInterface::ResponsesHttp | RequestInterface::ResponsesWebSocket => {
            if interface == RequestInterface::ResponsesWebSocket {
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
        RequestInterface::ImagesGeneration | RequestInterface::ImagesEdit => {
            set.insert("accept".into(), json!("application/json"));
            set.insert("content-type".into(), json!("application/json"));
            let turn = m["headers"]["x-codex-image-turn-id"]
                .as_str()
                .map(str::trim)
                .filter(|v| !v.is_empty() && v.len() <= 512)
                .unwrap_or(&identity.turn_id);
            set.insert("x-codex-image-turn-id".into(), json!(turn));
        }
        RequestInterface::StandaloneWebSearch => {
            set.insert("accept".into(), json!("application/json"));
            set.insert("content-type".into(), json!("application/json"));
            remove.push("x-codex-image-turn-id");
        }
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
    request_policy::check_image_edit_fields(fields)?;
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
    let gif = get("output_format")? == Some("gif");
    if gif {
        object.insert("output_format".into(), json!("gif"));
    }
    for (name, range) in request_policy::GIF_CONTROLS {
        if let Some(value) = get(name)? {
            let frames = value
                .parse::<u64>()
                .ok()
                .filter(|n| gif && range.contains(n))
                .ok_or_else(|| error("codex_request_body_field_value_unsupported"))?;
            object.insert(name.into(), json!(frames));
        }
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

    const OPERATIONS: [(&str, &str); 5] = [
        ("responses", "sse"),
        ("responses-ws", "websocket"),
        ("web_search", "non_stream"),
        ("images_generation", "non_stream"),
        ("images_edit", "non_stream"),
    ];

    #[test]
    fn descriptors_are_bounded_and_reuse_general_for_actual_upstream_usage() {
        for (operation, protocol) in OPERATIONS {
            let metadata = json!({"operation":operation});
            let descriptor = dispatch("attempt.describe/v1", metadata.clone(), &[]).unwrap();
            let legacy = dispatch("attempt.capabilities", metadata, &[]).unwrap();
            assert!(descriptor.body.is_empty());
            assert_eq!(descriptor.metadata.as_object().unwrap().len(), 3);
            assert_eq!(descriptor.metadata["capabilities"], legacy.metadata);
            for flag in [
                "preserves_affinity_on_failure",
                "successful_response_is_sse",
                "changes_request_body",
            ] {
                assert!(descriptor.metadata["capabilities"][flag].is_boolean());
            }
            assert_eq!(
                descriptor.metadata["protocols"],
                json!([{"protocol":protocol,"response":"passthrough"}])
            );
            assert_eq!(
                descriptor.metadata["usage"],
                json!({"parser":"general","format":"open_ai_responses"})
            );
            assert!(
                serde_json::to_vec(&descriptor.metadata).unwrap().len()
                    <= ai_gateway_connector_sdk::MAX_METADATA_BYTES
            );
        }
        for operation in ["unknown", "chat_completion", ""] {
            assert!(dispatch("attempt.describe/v1", json!({"operation":operation}), &[]).is_err());
        }
        assert!(dispatch("attempt.describe/v1", json!({}), &[]).is_err());
        assert!(
            dispatch(
                "attempt.describe/v1",
                json!({"operation":"responses"}),
                b"unexpected",
            )
            .is_err()
        );
    }

    fn header_context(operation: &str, protocol: &str) -> Value {
        let settings = Settings {
            originator: "codex_gateway".into(),
            client_version: "9.8.7".into(),
            user_agent: "codex_gateway/9.8.7".into(),
            ..Settings::default()
        };
        let identity = RequestIdentity::prepare(&json!({"credential_id":"11111111-1111-4111-8111-111111111111","request_id":"22222222-2222-4222-8222-222222222222","headers":{"session-id":"session","thread-id":"thread"},"operation":operation})).unwrap();
        json!({"operation":operation,"protocol":protocol,
            "access_token":"access-token","account_id":"account-123","is_fedramp":false,
            "settings":settings,"request_context":identity,"headers":{}})
    }

    #[test]
    fn header_plans_preserve_provider_auth_identity_and_transport_rules() {
        for (operation, protocol) in OPERATIONS {
            let mut metadata = header_context(operation, protocol);
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
    fn body_and_header_plans_reject_undeclared_transports() {
        for (operation, protocol, code) in [
            ("responses", "websocket", "invalid_request_body"),
            ("responses-ws", "sse", "invalid_request_body"),
            ("responses", "non_stream", "streaming_required"),
            ("web_search", "sse", "search_streaming_unsupported"),
            ("images_generation", "sse", "image_streaming_unsupported"),
            ("images_edit", "websocket", "invalid_request_body"),
        ] {
            let metadata = header_context(operation, protocol);
            for (command, body) in [
                ("attempt.body", b"{}".as_slice()),
                ("attempt.headers", &[][..]),
            ] {
                assert_eq!(
                    dispatch(command, metadata.clone(), body).unwrap_err().code,
                    code,
                    "{command} {operation}/{protocol}"
                );
            }
        }
    }

    #[test]
    fn caller_routing_hint_is_not_forwarded() {
        for (op, protocol) in [("responses", "sse"), ("responses-ws", "websocket")] {
            let mut m = header_context(op, protocol);
            m["headers"]["x-codex-routing-hint"] = json!("model=client;tier=priority");
            let plan = headers(&m).unwrap();
            assert!(
                plan["remove"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("x-codex-routing-hint"))
            );
            assert!(plan["set"].get("x-codex-routing-hint").is_none());
        }
    }

    #[test]
    fn responses_forward_only_supported_prompt_cache_options() {
        for (op, protocol) in [("responses", "sse"), ("responses-ws", "websocket")] {
            let metadata = header_context(op, protocol);
            let adapt = |options: Value| {
                dispatch(
                    "attempt.body",
                    metadata.clone(),
                    &serde_json::to_vec(&json!({"prompt_cache_options":options})).unwrap(),
                )
            };
            for ttl in ["30m", "12h"] {
                let options = json!({"ttl":ttl});
                let body: Value =
                    serde_json::from_slice(&adapt(options.clone()).unwrap().body).unwrap();
                assert_eq!(body["prompt_cache_options"], options);
            }
            let body: Value = serde_json::from_slice(&adapt(Value::Null).unwrap().body).unwrap();
            assert!(body.get("prompt_cache_options").is_none());
            for invalid in [
                json!({}),
                json!("30m"),
                json!({"ttl":30}),
                json!({"ttl":"24h"}),
                json!({"ttl":"12h","other":null}),
            ] {
                assert!(adapt(invalid).is_err());
            }
        }
    }

    #[test]
    fn image_turn_defaults_only_when_caller_has_no_valid_turn() {
        for turn in [Value::Null, json!(" "), json!("x".repeat(513))] {
            let mut context = header_context("images_generation", "non_stream");
            context["headers"]["x-codex-image-turn-id"] = turn;
            assert_eq!(
                headers(&context).unwrap()["set"]["x-codex-image-turn-id"],
                "22222222-2222-4222-8222-222222222222"
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
        let output = dispatch("attempt.body", header_context("responses", "sse"), raw).unwrap();
        let output: Value = serde_json::from_slice(&output.body).unwrap();
        assert_eq!(output["store"], false);
        assert_eq!(output["client_metadata"]["session_id"], "session");
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
    fn image_commands_forward_gif_controls_as_integer_siblings() {
        let plan = |extra: &[(&str, &str)]| {
            let fields = [("model", "gpt-image"), ("prompt", "animate")]
                .iter()
                .chain(extra)
                .map(|(name, value)| json!({"name":name,"value":value}))
                .collect::<Vec<_>>();
            dispatch(
                "attempt.image_edit_plan",
                json!({"image_count":1}),
                &serde_json::to_vec(&fields).unwrap(),
            )
        };
        let output = plan(&[
            ("output_format", "gif"),
            ("frame_count", "2"),
            ("frame_duration_ms", "2000"),
        ])
        .unwrap();
        let edit: Value = serde_json::from_slice(&output.body).unwrap();
        let generation = dispatch("attempt.body", header_context("images_generation", "non_stream"),
            br#"{"model":"gpt-image","prompt":"animate","output_format":"gif","frame_count":2,"frame_duration_ms":2000,"moderation":"auto"}"#).unwrap();
        let generation: Value = serde_json::from_slice(&generation.body).unwrap();
        for body in [edit, generation] {
            assert_eq!(body["output_format"], "gif");
            assert_eq!(body["frame_count"], 2);
            assert_eq!(body["frame_duration_ms"], 2000);
        }
        for fields in [
            vec![("frame_duration_ms", "20")],
            vec![("output_format", "png"), ("frame_count", "2")],
            vec![("output_format", "gif"), ("frame_count", "2.5")],
            vec![("output_format", "gif"), ("frame_count", "17")],
            vec![("output_format", "gif"), ("frame_duration_ms", "19")],
            vec![
                ("output_format", "gif"),
                ("frame_count", "2"),
                ("frame_count", "3"),
            ],
        ] {
            assert!(plan(&fields).is_err());
        }
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
