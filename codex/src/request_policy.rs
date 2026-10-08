//! Codex-owned outbound policy and privacy normalization.

use ai_gateway_connector_sdk::PluginCallError;
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::{collections::BTreeMap, fmt::Write as _, sync::LazyLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestInterface {
    ResponsesHttp,
    ResponsesWebSocket,
    StandaloneWebSearch,
    ImagesGeneration,
    ImagesEdit,
}
impl RequestInterface {
    pub fn from_metadata(m: &Value) -> Result<Self, PluginCallError> {
        match m["operation"].as_str() {
            Some("responses" | "responses-ws") if m["protocol"] == "websocket" => {
                Ok(Self::ResponsesWebSocket)
            }
            Some("responses" | "responses-ws") => Ok(Self::ResponsesHttp),
            Some("web_search") => Ok(Self::StandaloneWebSearch),
            Some("images_generation") => Ok(Self::ImagesGeneration),
            Some("images_edit") => Ok(Self::ImagesEdit),
            _ => Err(PluginCallError::new(
                "unsupported_operation",
                "Unsupported operation",
            )),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::ResponsesHttp => "responses_http",
            Self::ResponsesWebSocket => "responses_websocket",
            Self::StandaloneWebSearch => "standalone_web_search",
            Self::ImagesGeneration => "images_generation",
            Self::ImagesEdit => "images_edit",
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RequestPolicyLayer {
    CodexOauth,
}
#[derive(Clone, Copy)]
enum RequestPolicyLocation {
    Header,
    Body,
}
#[derive(Clone, Copy)]
enum RequestPolicyFailure {
    UnsupportedField,
    UnsupportedValue,
}
#[derive(Debug)]
pub(crate) struct RequestPolicyError {
    code: &'static str,
}
impl RequestPolicyError {
    #[cfg(test)]
    fn code(&self) -> &str {
        self.code
    }
    fn invalid_body(_: RequestPolicyLayer, _: RequestInterface) -> Self {
        Self {
            code: "invalid_request_body",
        }
    }
    fn field(
        _: RequestPolicyLayer,
        _: RequestInterface,
        location: RequestPolicyLocation,
        failure: RequestPolicyFailure,
        _: &str,
    ) -> Self {
        Self {
            code: match (location, failure) {
                (RequestPolicyLocation::Header, RequestPolicyFailure::UnsupportedField) => {
                    "codex_request_header_unsupported"
                }
                (RequestPolicyLocation::Header, RequestPolicyFailure::UnsupportedValue) => {
                    "codex_request_header_value_unsupported"
                }
                (RequestPolicyLocation::Body, RequestPolicyFailure::UnsupportedField) => {
                    "codex_request_body_field_unsupported"
                }
                (RequestPolicyLocation::Body, RequestPolicyFailure::UnsupportedValue) => {
                    "codex_request_body_field_value_unsupported"
                }
            },
        }
    }
}
impl From<RequestPolicyError> for PluginCallError {
    fn from(e: RequestPolicyError) -> Self {
        Self::new(e.code, "Codex request policy rejected the request")
    }
}
#[derive(Debug)]
pub(crate) struct AppliedJsonBody {
    pub body: Bytes,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FieldDisposition {
    Allow,
    Ignore,
}
pub(crate) fn apply_json_body_policy(
    layer: RequestPolicyLayer,
    interface: RequestInterface,
    body: Bytes,
) -> Result<AppliedJsonBody, RequestPolicyError> {
    let policy = body_policy(layer, interface);
    let raw_fields = serde_json::from_slice::<BTreeMap<String, &RawValue>>(&body)
        .map_err(|_| RequestPolicyError::invalid_body(layer, interface))?;
    let mut ignored_fields = Vec::new();
    let mut changed = false;
    for (field, raw_value) in &raw_fields {
        let inspected_value = policy
            .ignore
            .get(field)
            .and_then(|rule| rule.accepted_values.as_ref())
            .map(|_| {
                serde_json::from_str::<Value>(raw_value.get())
                    .map_err(|_| RequestPolicyError::invalid_body(layer, interface))
            })
            .transpose()?;
        let disposition = field_disposition_for_policy(
            layer,
            interface,
            policy,
            field,
            inspected_value.as_ref(),
        )?;
        if disposition == FieldDisposition::Ignore {
            ignored_fields.push(field.clone());
            changed = true;
        }
    }
    if layer == RequestPolicyLayer::CodexOauth {
        for (field, override_value) in &connector_policy(interface).body_overrides {
            let current_value = raw_fields
                .get(field)
                .and_then(|raw| serde_json::from_str::<Value>(raw.get()).ok());
            if current_value.as_ref() != Some(override_value) {
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(AppliedJsonBody { body });
    }
    let mut value = serde_json::from_slice::<Value>(&body)
        .map_err(|_| RequestPolicyError::invalid_body(layer, interface))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| RequestPolicyError::invalid_body(layer, interface))?;
    for field in ignored_fields {
        object.remove(&field);
    }
    if layer == RequestPolicyLayer::CodexOauth {
        for (field, override_value) in &connector_policy(interface).body_overrides {
            object.insert(field.clone(), override_value.clone());
        }
    }
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| RequestPolicyError::invalid_body(layer, interface))?;
    Ok(AppliedJsonBody { body })
}

#[derive(Clone, Debug)]
pub(crate) struct CodexRequestMetadata {
    installation_id: String,
    session_id: String,
    thread_id: String,
    turn_id: String,
    window_id: String,
    workspace_path: String,
    git_remote_url: String,
}

impl CodexRequestMetadata {
    #[must_use]
    pub(crate) fn new(
        installation_id: String,
        session_id: String,
        thread_id: String,
        turn_id: String,
        window_id: String,
        workspace_path: String,
        git_remote_url: String,
    ) -> Self {
        Self {
            installation_id,
            session_id,
            thread_id,
            turn_id,
            window_id,
            workspace_path,
            git_remote_url,
        }
    }
}

pub(crate) fn normalize_codex_fingerprints_in_json(
    interface: RequestInterface,
    body: Bytes,
    metadata: &CodexRequestMetadata,
) -> Result<AppliedJsonBody, RequestPolicyError> {
    if !matches!(
        interface,
        RequestInterface::ResponsesHttp | RequestInterface::ResponsesWebSocket
    ) {
        return Ok(AppliedJsonBody { body });
    }
    let policy = &contract().codex_fingerprint_normalization;
    let mut value = serde_json::from_slice::<Value>(&body)
        .map_err(|_| RequestPolicyError::invalid_body(RequestPolicyLayer::CodexOauth, interface))?;
    let object = value.as_object_mut().ok_or_else(|| {
        RequestPolicyError::invalid_body(RequestPolicyLayer::CodexOauth, interface)
    })?;
    let mut changed = false;
    if object
        .get(&policy.client_metadata_body_field)
        .is_none_or(Value::is_null)
    {
        object.insert(
            policy.client_metadata_body_field.clone(),
            Value::Object(serde_json::Map::new()),
        );
        changed = true;
    }
    let client_metadata = object
        .get_mut(&policy.client_metadata_body_field)
        .expect("Codex client_metadata was inserted when missing")
        .as_object_mut()
        .ok_or_else(|| {
            RequestPolicyError::field(
                RequestPolicyLayer::CodexOauth,
                interface,
                RequestPolicyLocation::Body,
                RequestPolicyFailure::UnsupportedValue,
                &policy.client_metadata_body_field,
            )
        })?;
    changed |= normalize_codex_client_metadata(client_metadata, metadata);
    let prompt_cache_key_missing = object
        .get(&policy.prompt_cache_key_body_field)
        .and_then(Value::as_str)
        .is_none_or(|value| value.trim().is_empty());
    if prompt_cache_key_missing {
        object.insert(
            policy.prompt_cache_key_body_field.clone(),
            Value::String(metadata.session_id.clone()),
        );
        changed = true;
    }
    if !changed {
        return Ok(AppliedJsonBody { body });
    }

    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| RequestPolicyError::invalid_body(RequestPolicyLayer::CodexOauth, interface))?;
    Ok(AppliedJsonBody { body })
}

pub(crate) fn normalize_codex_fingerprints_in_headers(
    interface: RequestInterface,
    headers: &mut HeaderMap,
    metadata: &CodexRequestMetadata,
) {
    if !matches!(
        interface,
        RequestInterface::ResponsesHttp
            | RequestInterface::ResponsesWebSocket
            | RequestInterface::StandaloneWebSearch
            | RequestInterface::ImagesGeneration
            | RequestInterface::ImagesEdit
    ) {
        return;
    }
    let policy = &contract().codex_fingerprint_normalization;
    for name in &policy.turn_metadata.header_fields {
        let name = HeaderName::from_bytes(name.as_bytes())
            .expect("validated Codex fingerprint header name must remain valid");
        let raw = headers
            .get(&name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        headers.remove(&name);
        let Some(normalized) = normalize_codex_turn_metadata(
            raw.as_deref(),
            metadata,
            interface != RequestInterface::ResponsesWebSocket,
        ) else {
            continue;
        };
        if let Ok(value) = HeaderValue::from_str(&normalized) {
            headers.insert(name, value);
        }
    }
    let name = HeaderName::from_bytes(policy.window_header_field.as_bytes())
        .expect("validated Codex window header name must remain valid");
    if headers
        .get(&name)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| value.trim().is_empty())
        && let Ok(value) = HeaderValue::from_str(&metadata.window_id)
    {
        headers.insert(name, value);
    }
}

fn normalize_codex_client_metadata(
    client_metadata: &mut serde_json::Map<String, Value>,
    metadata: &CodexRequestMetadata,
) -> bool {
    let policy = &contract().codex_fingerprint_normalization;
    let mut changed = false;
    for field in &policy.installation_id.client_metadata_fields {
        changed |= replace_string_field(client_metadata, field, &metadata.installation_id);
    }
    changed |= fill_string_field(client_metadata, "session_id", &metadata.session_id);
    changed |= fill_string_field(client_metadata, "thread_id", &metadata.thread_id);
    changed |= fill_string_field(client_metadata, "turn_id", &metadata.turn_id);
    changed |= fill_string_field(
        client_metadata,
        &policy.window_header_field,
        &metadata.window_id,
    );
    for field in &policy.turn_metadata.client_metadata_fields {
        let raw = client_metadata
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned);
        match normalize_codex_turn_metadata(raw.as_deref(), metadata, true) {
            Some(normalized) if raw.as_deref() != Some(normalized.as_str()) => {
                client_metadata.insert(field.clone(), Value::String(normalized));
                changed = true;
            }
            Some(_) => {}
            None => {}
        }
    }
    changed
}

fn normalize_codex_turn_metadata(
    raw: Option<&str>,
    metadata: &CodexRequestMetadata,
    fill_turn_id: bool,
) -> Option<String> {
    let policy = &contract().codex_fingerprint_normalization;
    let (mut object, mut changed) = match raw
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| value.as_object().cloned())
    {
        Some(object) => (object, false),
        None => (serde_json::Map::new(), true),
    };
    for field in &policy.installation_id.turn_metadata_fields {
        changed |= replace_string_field(&mut object, field, &metadata.installation_id);
    }
    changed |= fill_string_field(&mut object, "session_id", &metadata.session_id);
    changed |= fill_string_field(&mut object, "thread_id", &metadata.thread_id);
    if fill_turn_id {
        changed |= fill_string_field(&mut object, "turn_id", &metadata.turn_id);
    }
    changed |= fill_string_field(&mut object, "window_id", &metadata.window_id);
    let workspaces = codex_workspaces(metadata);
    if object.get(&policy.turn_metadata.workspace_field) != Some(&workspaces) {
        object.insert(policy.turn_metadata.workspace_field.clone(), workspaces);
        changed = true;
    }
    if !changed {
        return raw.map(str::to_owned);
    }
    serialize_ascii_json(&Value::Object(object))
}

fn replace_string_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: &str,
) -> bool {
    let replacement = Value::String(value.to_owned());
    if object.get(field) == Some(&replacement) {
        return false;
    }
    object.insert(field.to_owned(), replacement);
    true
}

fn fill_string_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: &str,
) -> bool {
    if object
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return false;
    }
    object.insert(field.to_owned(), Value::String(value.to_owned()));
    true
}

fn codex_workspaces(metadata: &CodexRequestMetadata) -> Value {
    let mut remote_urls = serde_json::Map::new();
    remote_urls.insert(
        "origin".into(),
        Value::String(metadata.git_remote_url.clone()),
    );
    let mut workspace = serde_json::Map::new();
    workspace.insert("associated_remote_urls".into(), Value::Object(remote_urls));
    let mut workspaces = serde_json::Map::new();
    workspaces.insert(metadata.workspace_path.clone(), Value::Object(workspace));
    Value::Object(workspaces)
}

fn serialize_ascii_json(value: &Value) -> Option<String> {
    let serialized = serde_json::to_string(value).ok()?;
    if serialized.is_ascii() {
        return Some(serialized);
    }
    let mut ascii = String::with_capacity(serialized.len());
    for character in serialized.chars() {
        if character.is_ascii() {
            ascii.push(character);
            continue;
        }
        let mut encoded = [0; 2];
        for unit in character.encode_utf16(&mut encoded) {
            write!(&mut ascii, "\\u{unit:04x}").ok()?;
        }
    }
    Some(ascii)
}

fn field_disposition_for_policy(
    layer: RequestPolicyLayer,
    interface: RequestInterface,
    policy: &BodyPolicy,
    field: &str,
    value: Option<&Value>,
) -> Result<FieldDisposition, RequestPolicyError> {
    if contains_sorted(&policy.allow, field) {
        return Ok(FieldDisposition::Allow);
    }
    if let Some(rule) = policy.ignore.get(field) {
        if rule.accepted_values.as_ref().is_some_and(|accepted| {
            value.is_none_or(|value| !accepted.iter().any(|candidate| candidate == value))
        }) {
            return Err(RequestPolicyError::field(
                layer,
                interface,
                RequestPolicyLocation::Body,
                RequestPolicyFailure::UnsupportedValue,
                field,
            ));
        }
        return Ok(FieldDisposition::Ignore);
    }
    if contains_sorted(&policy.reject, field) {
        return Err(RequestPolicyError::field(
            layer,
            interface,
            RequestPolicyLocation::Body,
            RequestPolicyFailure::UnsupportedField,
            field,
        ));
    }
    match policy.unknown {
        UnknownAction::Allow => Ok(FieldDisposition::Allow),
        UnknownAction::Ignore => Ok(FieldDisposition::Ignore),
        UnknownAction::Reject => Err(RequestPolicyError::field(
            layer,
            interface,
            RequestPolicyLocation::Body,
            RequestPolicyFailure::UnsupportedField,
            field,
        )),
    }
}

fn header_action(policy: &HeaderPolicy, name: &HeaderName) -> UnknownAction {
    let name = name.as_str();
    if contains_sorted(&policy.allow, name) {
        UnknownAction::Allow
    } else if contains_sorted(&policy.ignore, name) {
        UnknownAction::Ignore
    } else if policy
        .allow_prefixes
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        UnknownAction::Allow
    } else {
        policy.unknown
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexFingerprintNormalizationPolicy {
    client_metadata_body_field: String,
    prompt_cache_key_body_field: String,
    window_header_field: String,
    installation_id: CodexInstallationIdNormalizationPolicy,
    turn_metadata: CodexTurnMetadataNormalizationPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexInstallationIdNormalizationPolicy {
    #[serde(rename = "scope")]
    _scope: CodexFingerprintScope,
    client_metadata_fields: Vec<String>,
    turn_metadata_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexTurnMetadataNormalizationPolicy {
    client_metadata_fields: Vec<String>,
    header_fields: Vec<String>,
    workspace_field: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CodexFingerprintScope {
    Credential,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InterfacePolicy {
    codex_oauth: ConnectorPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorPolicy {
    headers: HeaderPolicy,
    body: BodyPolicy,
    body_overrides: BTreeMap<String, Value>,
    #[serde(rename = "generated_body_fields")]
    _generated_body_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderPolicy {
    unknown: UnknownAction,
    allow: Vec<String>,
    allow_prefixes: Vec<String>,
    ignore: Vec<String>,
    #[serde(rename = "generated")]
    _generated: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyPolicy {
    unknown: UnknownAction,
    allow: Vec<String>,
    ignore: BTreeMap<String, IgnoredFieldRule>,
    reject: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IgnoredFieldRule {
    #[serde(default)]
    accepted_values: Option<Vec<Value>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum UnknownAction {
    Allow,
    Ignore,
    Reject,
}

#[derive(Deserialize)]
struct Contract {
    codex_fingerprint_normalization: CodexFingerprintNormalizationPolicy,
    interfaces: BTreeMap<String, InterfacePolicy>,
}
static CONTRACT: LazyLock<Contract> = LazyLock::new(|| {
    serde_json::from_str(include_str!("request-allowlists.json"))
        .expect("validated embedded Codex policy")
});
fn contract() -> &'static Contract {
    &CONTRACT
}
fn connector_policy(i: RequestInterface) -> &'static ConnectorPolicy {
    &contract().interfaces[i.as_str()].codex_oauth
}
fn body_policy(_: RequestPolicyLayer, i: RequestInterface) -> &'static BodyPolicy {
    &connector_policy(i).body
}
fn contains_sorted(v: &[String], s: &str) -> bool {
    v.binary_search_by(|v| v.as_str().cmp(s)).is_ok()
}
pub(crate) fn filter_codex_headers(
    i: RequestInterface,
    headers: &HeaderMap,
) -> Result<HeaderMap, RequestPolicyError> {
    let mut out = HeaderMap::new();
    for (name, value) in headers {
        match header_action(&connector_policy(i).headers, name) {
            UnknownAction::Allow => {
                out.append(name.clone(), value.clone());
            }
            UnknownAction::Ignore => {}
            UnknownAction::Reject => {
                return Err(RequestPolicyError::field(
                    RequestPolicyLayer::CodexOauth,
                    i,
                    RequestPolicyLocation::Header,
                    RequestPolicyFailure::UnsupportedField,
                    name.as_str(),
                ));
            }
        }
    }
    Ok(out)
}
pub(crate) fn check_image_edit_fields(fields: &[Value]) -> Result<(), PluginCallError> {
    for field in fields {
        let name = field["name"]
            .as_str()
            .ok_or_else(|| PluginCallError::new("invalid_field", "Invalid field"))?;
        let value = field["value"].as_str().unwrap_or("");
        let parsed = Value::String(value.into());
        field_disposition_for_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ImagesEdit,
            body_policy(RequestPolicyLayer::CodexOauth, RequestInterface::ImagesEdit),
            name,
            Some(&parsed),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn codex_metadata() -> CodexRequestMetadata {
        CodexRequestMetadata::new(
            "11111111-1111-4111-8111-111111111111".into(),
            "session-123".into(),
            "thread-456".into(),
            "turn-789".into(),
            "thread-456:0".into(),
            "/workspace".into(),
            "https://github.com/oai404iao/ai_gateway".into(),
        )
    }

    #[test]
    fn codex_fingerprint_normalization_replaces_private_fields_and_fills_safe_metadata() {
        let raw_turn_metadata = json!({
            "installation_id": "client-installation",
            "session_id": "session-123",
            "thread_id": "thread-456",
            "request_kind": "turn",
            "sandbox": "seatbelt",
            "workspaces": {
                "/home/alice/private-repo": {
                    "associated_remote_urls": {
                        "origin": "git@github.com:alice/private-repo.git"
                    },
                    "latest_git_commit_hash": "abc123",
                    "has_changes": true
                }
            },
            "app_server_extra": "保留"
        })
        .to_string();
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "gpt-5-codex",
                "input": [],
                "client_metadata": {
                    "x-codex-installation-id": "client-installation",
                    "session_id": "session-123",
                    "x-codex-turn-metadata": raw_turn_metadata
                }
            }))
            .unwrap(),
        );
        let metadata = codex_metadata();

        let normalized =
            normalize_codex_fingerprints_in_json(RequestInterface::ResponsesHttp, body, &metadata)
                .unwrap();
        let normalized: Value = serde_json::from_slice(&normalized.body).unwrap();
        let client_metadata = normalized["client_metadata"].as_object().unwrap();
        assert_eq!(
            client_metadata["x-codex-installation-id"],
            metadata.installation_id
        );
        assert_eq!(client_metadata["session_id"], "session-123");
        assert_eq!(client_metadata["thread_id"], "thread-456");
        assert_eq!(client_metadata["turn_id"], "turn-789");
        assert_eq!(client_metadata["x-codex-window-id"], "thread-456:0");
        assert_eq!(normalized["prompt_cache_key"], "session-123");
        let raw_turn_metadata = client_metadata["x-codex-turn-metadata"].as_str().unwrap();
        assert!(raw_turn_metadata.is_ascii());
        let turn_metadata: Value = serde_json::from_str(raw_turn_metadata).unwrap();
        assert_eq!(turn_metadata["installation_id"], metadata.installation_id);
        assert_eq!(turn_metadata["session_id"], "session-123");
        assert_eq!(turn_metadata["thread_id"], "thread-456");
        assert_eq!(turn_metadata["request_kind"], "turn");
        assert_eq!(turn_metadata["sandbox"], "seatbelt");
        assert_eq!(
            turn_metadata["workspaces"],
            json!({
                "/workspace": {
                    "associated_remote_urls": {
                        "origin": "https://github.com/oai404iao/ai_gateway"
                    }
                }
            })
        );
        assert_eq!(turn_metadata["app_server_extra"], "保留");
    }

    #[test]
    fn standalone_identity_enrichment_is_header_only_and_preserves_body_bytes() {
        for (interface, body) in [
            (
                RequestInterface::StandaloneWebSearch,
                Bytes::from_static(br#"{ "model":"search", "id":"caller-search", "commands":{} }"#),
            ),
            (
                RequestInterface::ImagesGeneration,
                Bytes::from_static(br#"{ "model":"image", "prompt":"a red hat" }"#),
            ),
            (
                RequestInterface::ImagesEdit,
                Bytes::from_static(br#"{ "model":"image", "images":[{"image_url":"data:image/png;base64,aA=="}] }"#),
            ),
        ] {
            let applied =
                normalize_codex_fingerprints_in_json(interface, body.clone(), &codex_metadata())
                    .unwrap();
            assert_eq!(applied.body, body);
        }
    }

    #[test]
    fn codex_fingerprint_normalization_updates_and_synthesizes_search_headers() {
        let metadata = codex_metadata();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_str(
                &json!({
                    "installation_id": "client-installation",
                    "workspaces": {"/private/repo": {"has_changes": true}},
                    "request_kind": "turn"
                })
                .to_string(),
            )
            .unwrap(),
        );
        headers.insert("traceparent", HeaderValue::from_static("preserve-trace"));

        normalize_codex_fingerprints_in_headers(
            RequestInterface::StandaloneWebSearch,
            &mut headers,
            &metadata,
        );

        let turn_metadata: Value = serde_json::from_str(
            headers
                .get("x-codex-turn-metadata")
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(turn_metadata["installation_id"], metadata.installation_id);
        assert_eq!(turn_metadata["session_id"], "session-123");
        assert_eq!(turn_metadata["thread_id"], "thread-456");
        assert_eq!(turn_metadata["turn_id"], "turn-789");
        assert_eq!(turn_metadata["window_id"], "thread-456:0");
        assert_eq!(
            turn_metadata["workspaces"],
            json!({
                "/workspace": {
                    "associated_remote_urls": {
                        "origin": "https://github.com/oai404iao/ai_gateway"
                    }
                }
            })
        );
        assert_eq!(turn_metadata["request_kind"], "turn");
        assert_eq!(headers.get("traceparent").unwrap(), "preserve-trace");

        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static("not-json"),
        );
        normalize_codex_fingerprints_in_headers(
            RequestInterface::StandaloneWebSearch,
            &mut headers,
            &metadata,
        );
        let turn_metadata: Value = serde_json::from_str(
            headers
                .get("x-codex-turn-metadata")
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(turn_metadata["installation_id"], metadata.installation_id);
        assert_eq!(
            turn_metadata["workspaces"]["/workspace"]["associated_remote_urls"]["origin"],
            "https://github.com/oai404iao/ai_gateway"
        );
    }

    #[test]
    fn codex_json_policy_uses_explicit_ignore_reject_and_override_rules() {
        let applied = apply_json_body_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ImagesGeneration,
            Bytes::from_static(
                br#"{"model":"gpt-image-2","prompt":"x","output_format":"png","moderation":"auto","user":"u"}"#,
            ),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&applied.body).unwrap();
        assert_eq!(value, json!({"model":"gpt-image-2","prompt":"x"}));

        let error = apply_json_body_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ImagesGeneration,
            Bytes::from_static(br#"{"model":"gpt-image-2","prompt":"x","output_format":"jpeg"}"#),
        )
        .unwrap_err();
        assert_eq!(error.code(), "codex_request_body_field_value_unsupported");

        let applied = apply_json_body_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ResponsesHttp,
            Bytes::from_static(
                br#"{"model":"gpt-5-codex","input":[],"max_output_tokens":1,"stream":true,"store":true}"#,
            ),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&applied.body).unwrap();
        assert!(value.get("max_output_tokens").is_none());
        assert_eq!(value["stream"], true);
        assert_eq!(value["store"], false);

        let error = apply_json_body_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ResponsesHttp,
            Bytes::from_static(
                br#"{"model":"gpt-5-codex","input":[],"stream":true,"previous_response_id":"resp_1"}"#,
            ),
        )
        .unwrap_err();
        assert_eq!(error.code(), "codex_request_body_field_value_unsupported");

        let error = apply_json_body_policy(
            RequestPolicyLayer::CodexOauth,
            RequestInterface::ResponsesHttp,
            Bytes::from_static(
                br#"{"model":"gpt-5-codex","input":[],"stream":true,"future_field":true}"#,
            ),
        )
        .unwrap_err();
        assert_eq!(error.code(), "codex_request_body_field_unsupported");
    }
}
