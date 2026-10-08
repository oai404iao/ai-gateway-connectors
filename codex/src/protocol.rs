//! Pure Codex control-plane request plans and provider response parsing.
use ai_gateway_connector_sdk::{PluginCallError, PluginOutput};
use base64::Engine;
use chrono::{DateTime, Utc};
use http::{
    StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

#[derive(Debug, Serialize)]
enum CodexConnectorError {
    InvalidEndpoint,
    InvalidCallback,
    OauthDenied,
    InvalidCredential,
    InvalidJwt,
    InvalidTokenResponse,
    TokenEndpointStatus(u16),
    RefreshTokenInvalid,
    CodexBackendStatus(u16),
    InvalidModelsResponse,
    NoModels,
    InvalidQuotaResponse,
}
#[derive(Clone, Deserialize)]
struct CodexOutboundIdentity {
    originator: String,
    client_version: String,
    user_agent: String,
}
impl CodexOutboundIdentity {
    fn originator(&self) -> &str {
        &self.originator
    }
    fn client_version(&self) -> &str {
        &self.client_version
    }
    fn user_agent(&self) -> &str {
        &self.user_agent
    }
}
#[derive(Serialize)]
struct CodexQuotaUpdate {
    allowed: bool,
    limit_reached: bool,
    primary_used_percent: Option<i32>,
    primary_window_seconds: Option<i32>,
    primary_reset_at: Option<DateTime<Utc>>,
    secondary_used_percent: Option<i32>,
    secondary_window_seconds: Option<i32>,
    secondary_reset_at: Option<DateTime<Utc>>,
    reset_credits_available: Option<i64>,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CodexQuotaResetOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_OAUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CODEX_OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CodexEndpoints {
    issuer: Url,
    responses_base_url: Url,
}

impl Default for CodexEndpoints {
    fn default() -> Self {
        Self {
            issuer: Url::parse("https://auth.openai.com").expect("Codex issuer URL is valid"),
            responses_base_url: Url::parse("https://chatgpt.com/backend-api/codex")
                .expect("Codex Responses base URL is valid"),
        }
    }
}

impl CodexEndpoints {
    fn token_url(&self) -> Result<Url, CodexConnectorError> {
        self.issuer
            .join("/oauth/token")
            .map_err(|_| CodexConnectorError::InvalidEndpoint)
    }

    fn models_url(&self, identity: &CodexOutboundIdentity) -> Result<Url, CodexConnectorError> {
        let mut url = Url::parse(&format!(
            "{}/models",
            self.responses_base_url.as_str().trim_end_matches('/')
        ))
        .map_err(|_| CodexConnectorError::InvalidEndpoint)?;
        url.query_pairs_mut()
            .append_pair("client_version", identity.client_version());
        Ok(url)
    }

    fn quota_url(&self) -> Result<Url, CodexConnectorError> {
        let base = self.responses_base_url.as_str().trim_end_matches('/');
        let backend = base
            .strip_suffix("/codex")
            .ok_or(CodexConnectorError::InvalidEndpoint)?;
        Url::parse(&format!("{backend}/wham/usage"))
            .map_err(|_| CodexConnectorError::InvalidEndpoint)
    }

    fn quota_reset_url(&self) -> Result<Url, CodexConnectorError> {
        let base = self.responses_base_url.as_str().trim_end_matches('/');
        let backend = base
            .strip_suffix("/codex")
            .ok_or(CodexConnectorError::InvalidEndpoint)?;
        Url::parse(&format!("{backend}/wham/rate-limit-reset-credits/consume"))
            .map_err(|_| CodexConnectorError::InvalidEndpoint)
    }
}

#[derive(Clone, Serialize)]
struct PkceCodes {
    challenge: String,
}

#[derive(Clone, Debug, Serialize)]
struct CodexIdentity {
    email: Option<String>,
    account_id: Option<String>,
    user_id: Option<String>,
    plan_type: Option<String>,
    is_fedramp: bool,
}

#[derive(Clone, Serialize)]
struct ExchangedTokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

#[derive(Clone, Serialize)]
struct RefreshedTokens {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Clone, Serialize)]
struct CallbackCode {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct QuotaUsageResponse {
    rate_limit: Option<QuotaRateLimit>,
    #[serde(default)]
    rate_limit_reset_credits: Option<QuotaResetCreditsSummary>,
}

#[derive(Deserialize)]
struct QuotaRateLimit {
    allowed: bool,
    limit_reached: bool,
    primary_window: Option<QuotaWindow>,
    secondary_window: Option<QuotaWindow>,
}

#[derive(Deserialize)]
struct QuotaWindow {
    used_percent: i32,
    limit_window_seconds: i32,
    reset_at: i64,
}

#[derive(Deserialize)]
struct QuotaResetCreditsSummary {
    available_count: i64,
}

#[derive(Serialize)]
struct ConsumeQuotaResetCreditRequest {
    redeem_request_id: String,
}

#[derive(Deserialize)]
struct ConsumeQuotaResetCreditResponse {
    code: ConsumeQuotaResetCreditCode,
    #[serde(default)]
    windows_reset: i32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConsumeQuotaResetCreditCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct CodexQuotaResetResult {
    outcome: CodexQuotaResetOutcome,
    windows_reset: i32,
}

fn build_authorize_url(
    endpoints: &CodexEndpoints,
    pkce: &PkceCodes,
    state: &str,
    identity: &CodexOutboundIdentity,
) -> Result<String, CodexConnectorError> {
    let mut url = endpoints
        .issuer
        .join("/oauth/authorize")
        .map_err(|_| CodexConnectorError::InvalidEndpoint)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CODEX_OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", CODEX_OAUTH_REDIRECT_URI)
        .append_pair("scope", CODEX_OAUTH_SCOPE)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", identity.originator());
    Ok(url.into())
}
fn parse_callback_url(value: &str) -> Result<CallbackCode, CodexConnectorError> {
    let url = Url::parse(value).map_err(|_| CodexConnectorError::InvalidCallback)?;
    if url.scheme() != "http"
        || url.host_str() != Some("localhost")
        || url.port_or_known_default() != Some(1455)
        || url.path() != "/auth/callback"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(CodexConnectorError::InvalidCallback);
    }
    let values = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    if values.contains_key("error") {
        return Err(CodexConnectorError::OauthDenied);
    }
    let code = values
        .get("code")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or(CodexConnectorError::InvalidCallback)?;
    let state = values
        .get("state")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or(CodexConnectorError::InvalidCallback)?;
    Ok(CallbackCode {
        code: code.to_owned(),
        state: state.to_owned(),
    })
}
fn parse_exchange(status: StatusCode, body: &[u8]) -> Result<ExchangedTokens, CodexConnectorError> {
    #[derive(Deserialize)]
    struct TokenResponse {
        id_token: String,
        access_token: String,
        refresh_token: String,
    }

    if !status.is_success() {
        return Err(token_endpoint_error(status, body, false));
    }
    let response: TokenResponse =
        serde_json::from_slice(body).map_err(|_| CodexConnectorError::InvalidTokenResponse)?;
    if response.id_token.is_empty()
        || response.access_token.is_empty()
        || response.refresh_token.is_empty()
    {
        return Err(CodexConnectorError::InvalidTokenResponse);
    }
    Ok(ExchangedTokens {
        id_token: response.id_token,
        access_token: response.access_token,
        refresh_token: response.refresh_token,
    })
}
fn parse_refresh(status: StatusCode, body: &[u8]) -> Result<RefreshedTokens, CodexConnectorError> {
    #[derive(Deserialize)]
    struct RefreshResponse {
        id_token: Option<String>,
        access_token: Option<String>,
        refresh_token: Option<String>,
    }

    if !status.is_success() {
        return Err(token_endpoint_error(status, body, true));
    }
    let response: RefreshResponse =
        serde_json::from_slice(body).map_err(|_| CodexConnectorError::InvalidTokenResponse)?;
    if response.id_token.is_none()
        && response.access_token.is_none()
        && response.refresh_token.is_none()
    {
        return Err(CodexConnectorError::InvalidTokenResponse);
    }
    Ok(RefreshedTokens {
        id_token: response.id_token.filter(|value| !value.is_empty()),
        access_token: response.access_token.filter(|value| !value.is_empty()),
        refresh_token: response.refresh_token.filter(|value| !value.is_empty()),
    })
}
fn parse_models(status: StatusCode, body: &[u8]) -> Result<Vec<String>, CodexConnectorError> {
    #[derive(Deserialize)]
    struct ModelsEnvelope {
        models: Vec<Model>,
    }
    #[derive(Deserialize)]
    struct Model {
        slug: String,
        #[serde(default)]
        supported_in_api: Option<bool>,
    }

    if !status.is_success() {
        return Err(CodexConnectorError::CodexBackendStatus(status.as_u16()));
    }
    let response: ModelsEnvelope =
        serde_json::from_slice(body).map_err(|_| CodexConnectorError::InvalidModelsResponse)?;
    let mut models = response
        .models
        .into_iter()
        .filter(|model| model.supported_in_api != Some(false))
        .map(|model| model.slug.trim().to_owned())
        .filter(|model| !model.is_empty() && model.len() <= 300)
        .collect::<Vec<_>>();
    models.sort_unstable();
    models.dedup();
    if models.is_empty() {
        return Err(CodexConnectorError::NoModels);
    }
    Ok(models)
}
fn parse_quota(status: StatusCode, body: &[u8]) -> Result<CodexQuotaUpdate, CodexConnectorError> {
    if !status.is_success() {
        return Err(CodexConnectorError::CodexBackendStatus(status.as_u16()));
    }
    let response: QuotaUsageResponse =
        serde_json::from_slice(body).map_err(|_| CodexConnectorError::InvalidQuotaResponse)?;
    let rate_limit = response
        .rate_limit
        .ok_or(CodexConnectorError::InvalidQuotaResponse)?;
    let primary = validate_window(rate_limit.primary_window)?;
    let secondary = validate_window(rate_limit.secondary_window)?;
    let reset_credits_available = response
        .rate_limit_reset_credits
        .map(|credits| credits.available_count)
        .map(|count| {
            if count < 0 {
                Err(CodexConnectorError::InvalidQuotaResponse)
            } else {
                Ok(count)
            }
        })
        .transpose()?;
    Ok(CodexQuotaUpdate {
        allowed: rate_limit.allowed,
        limit_reached: rate_limit.limit_reached,
        primary_used_percent: primary.as_ref().map(|window| window.used_percent),
        primary_window_seconds: primary.as_ref().map(|window| window.window_seconds),
        primary_reset_at: primary.as_ref().and_then(|window| window.reset_at),
        secondary_used_percent: secondary.as_ref().map(|window| window.used_percent),
        secondary_window_seconds: secondary.as_ref().map(|window| window.window_seconds),
        secondary_reset_at: secondary.as_ref().and_then(|window| window.reset_at),
        reset_credits_available,
    })
}
fn parse_quota_reset(
    status: StatusCode,
    body: &[u8],
) -> Result<CodexQuotaResetResult, CodexConnectorError> {
    if !status.is_success() {
        return Err(CodexConnectorError::CodexBackendStatus(status.as_u16()));
    }
    let response: ConsumeQuotaResetCreditResponse =
        serde_json::from_slice(body).map_err(|_| CodexConnectorError::InvalidQuotaResponse)?;
    if !(0..=2).contains(&response.windows_reset) {
        return Err(CodexConnectorError::InvalidQuotaResponse);
    }
    let outcome = match response.code {
        ConsumeQuotaResetCreditCode::Reset => CodexQuotaResetOutcome::Reset,
        ConsumeQuotaResetCreditCode::NothingToReset => CodexQuotaResetOutcome::NothingToReset,
        ConsumeQuotaResetCreditCode::NoCredit => CodexQuotaResetOutcome::NoCredit,
        ConsumeQuotaResetCreditCode::AlreadyRedeemed => CodexQuotaResetOutcome::AlreadyRedeemed,
    };
    Ok(CodexQuotaResetResult {
        outcome,
        windows_reset: response.windows_reset,
    })
}
struct ValidWindow {
    used_percent: i32,
    window_seconds: i32,
    reset_at: Option<DateTime<Utc>>,
}

fn validate_window(
    window: Option<QuotaWindow>,
) -> Result<Option<ValidWindow>, CodexConnectorError> {
    let Some(window) = window else {
        return Ok(None);
    };
    if !(0..=100).contains(&window.used_percent) || window.limit_window_seconds <= 0 {
        return Err(CodexConnectorError::InvalidQuotaResponse);
    }
    let reset_at = DateTime::from_timestamp(window.reset_at, 0)
        .ok_or(CodexConnectorError::InvalidQuotaResponse)?;
    Ok(Some(ValidWindow {
        used_percent: window.used_percent,
        window_seconds: window.limit_window_seconds,
        reset_at: Some(reset_at),
    }))
}

fn parse_identity(jwt: &str) -> Result<CodexIdentity, CodexConnectorError> {
    #[derive(Deserialize)]
    struct Claims {
        #[serde(default)]
        email: Option<String>,
        #[serde(default)]
        sub: Option<String>,
        #[serde(rename = "https://api.openai.com/profile", default)]
        profile: Option<Profile>,
        #[serde(rename = "https://api.openai.com/auth", default)]
        auth: Option<Auth>,
    }
    #[derive(Deserialize)]
    struct Profile {
        #[serde(default)]
        email: Option<String>,
    }
    #[derive(Deserialize)]
    struct Auth {
        #[serde(default)]
        chatgpt_plan_type: Option<Value>,
        #[serde(default)]
        chatgpt_account_id: Option<String>,
        #[serde(default)]
        chatgpt_user_id: Option<String>,
        #[serde(default)]
        user_id: Option<String>,
        #[serde(default)]
        chatgpt_account_is_fedramp: bool,
    }

    let claims: Claims = decode_jwt(jwt)?;
    let email = normalize_claim(
        claims
            .email
            .or_else(|| claims.profile.and_then(|profile| profile.email)),
        320,
    )?;
    let auth = claims.auth;
    let account_id = normalize_claim(
        auth.as_ref()
            .and_then(|auth| auth.chatgpt_account_id.clone()),
        300,
    )?;
    let user_id = normalize_claim(
        auth.as_ref()
            .and_then(|auth| {
                auth.chatgpt_user_id
                    .clone()
                    .or_else(|| auth.user_id.clone())
            })
            .or(claims.sub),
        300,
    )?;
    let plan_type = match auth
        .as_ref()
        .and_then(|auth| auth.chatgpt_plan_type.as_ref())
    {
        Some(Value::String(value)) => normalize_claim(Some(value.clone()), 100)?,
        Some(Value::Null) | None => None,
        Some(_) => return Err(CodexConnectorError::InvalidJwt),
    };
    Ok(CodexIdentity {
        email,
        account_id,
        user_id,
        plan_type,
        is_fedramp: auth
            .as_ref()
            .is_some_and(|auth| auth.chatgpt_account_is_fedramp),
    })
}

fn parse_jwt_expiration(jwt: &str) -> Result<Option<DateTime<Utc>>, CodexConnectorError> {
    #[derive(Deserialize)]
    struct Claims {
        #[serde(default)]
        exp: Option<i64>,
    }
    let claims: Claims = decode_jwt(jwt)?;
    claims.exp.map_or(Ok(None), |value| {
        DateTime::from_timestamp(value, 0)
            .map(Some)
            .ok_or(CodexConnectorError::InvalidJwt)
    })
}

fn decode_jwt<T: for<'de> Deserialize<'de>>(jwt: &str) -> Result<T, CodexConnectorError> {
    let mut parts = jwt.split('.');
    let (_, payload, _) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(signature), None)
            if !header.is_empty() && !payload.is_empty() && !signature.is_empty() =>
        {
            (header, payload, signature)
        }
        _ => return Err(CodexConnectorError::InvalidJwt),
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .map_err(|_| CodexConnectorError::InvalidJwt)?;
    serde_json::from_slice(&bytes).map_err(|_| CodexConnectorError::InvalidJwt)
}

fn normalize_claim(
    value: Option<String>,
    maximum_bytes: usize,
) -> Result<Option<String>, CodexConnectorError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > maximum_bytes {
        return Err(CodexConnectorError::InvalidJwt);
    }
    Ok(Some(value.to_owned()))
}

fn codex_headers(
    identity: &CodexOutboundIdentity,
    access_token: &str,
    account_id: Option<&str>,
    is_fedramp: bool,
) -> Result<HeaderMap, CodexConnectorError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .map_err(|_| CodexConnectorError::InvalidCredential)?,
    );
    if let Some(account_id) = account_id {
        headers.insert(
            "ChatGPT-Account-ID",
            HeaderValue::from_str(account_id)
                .map_err(|_| CodexConnectorError::InvalidCredential)?,
        );
    }
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(identity.user_agent())
            .map_err(|_| CodexConnectorError::InvalidCredential)?,
    );
    headers.insert(
        "originator",
        HeaderValue::from_str(identity.originator())
            .map_err(|_| CodexConnectorError::InvalidCredential)?,
    );
    headers.insert(
        "version",
        HeaderValue::from_str(identity.client_version())
            .map_err(|_| CodexConnectorError::InvalidCredential)?,
    );
    if is_fedramp {
        headers.insert("X-OpenAI-Fedramp", HeaderValue::from_static("true"));
    }
    Ok(headers)
}

fn token_endpoint_error(status: StatusCode, body: &[u8], refreshing: bool) -> CodexConnectorError {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| {
                    error
                        .get("code")
                        .and_then(Value::as_str)
                        .or_else(|| error.as_str())
                })
                .or_else(|| value.get("code").and_then(Value::as_str))
                .map(str::to_ascii_lowercase)
        });
    if refreshing
        && (status == StatusCode::UNAUTHORIZED
            || matches!(
                code.as_deref(),
                Some(
                    "refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated"
                )
            ))
    {
        CodexConnectorError::RefreshTokenInvalid
    } else {
        CodexConnectorError::TokenEndpointStatus(status.as_u16())
    }
}

fn field<'a>(metadata: &'a Value, name: &str) -> Result<&'a str, CodexConnectorError> {
    metadata
        .get(name)
        .and_then(Value::as_str)
        .ok_or(CodexConnectorError::InvalidCredential)
}
fn encode<T: Serialize>(value: T) -> Result<Value, CodexConnectorError> {
    serde_json::to_value(value).map_err(|_| CodexConnectorError::InvalidCredential)
}
fn process(
    command: &str,
    metadata: Value,
    body: &[u8],
) -> Result<PluginOutput, CodexConnectorError> {
    let output = |value| {
        Ok(PluginOutput {
            metadata: value,
            body: Vec::new(),
        })
    };
    match command {
        "endpoints" => {
            return output(json!({"issuer":CodexEndpoints::default().issuer,
            "responses_base_url":CodexEndpoints::default().responses_base_url,
            "redirect_uri":CODEX_OAUTH_REDIRECT_URI}));
        }
        "parse_callback" => return output(encode(parse_callback_url(field(&metadata, "url")?)?)?),
        "parse_identity" => return output(encode(parse_identity(field(&metadata, "token")?)?)?),
        "parse_expiration" => {
            return output(encode(parse_jwt_expiration(field(&metadata, "token")?)?)?);
        }
        _ => {}
    }
    if command.ends_with("_parse") {
        let status = metadata
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|v| u16::try_from(v).ok())
            .and_then(|v| StatusCode::from_u16(v).ok())
            .ok_or(CodexConnectorError::InvalidCredential)?;
        return output(match command {
            "exchange_parse" => encode(parse_exchange(status, body)?)?,
            "refresh_parse" => encode(parse_refresh(status, body)?)?,
            "models_parse" => encode(parse_models(status, body)?)?,
            "quota_parse" => encode(parse_quota(status, body)?)?,
            "quota_reset_parse" => encode(parse_quota_reset(status, body)?)?,
            _ => return Err(CodexConnectorError::InvalidCredential),
        });
    }
    let endpoints: CodexEndpoints = serde_json::from_value(metadata["endpoints"].clone())
        .map_err(|_| CodexConnectorError::InvalidEndpoint)?;
    if command == "authorize_url" {
        let identity = serde_json::from_value(metadata["settings"].clone())
            .map_err(|_| CodexConnectorError::InvalidCredential)?;
        let pkce = PkceCodes {
            challenge: field(&metadata, "challenge")?.into(),
        };
        return output(encode(build_authorize_url(
            &endpoints,
            &pkce,
            field(&metadata, "state")?,
            &identity,
        )?)?);
    }
    let mut headers = serde_json::Map::new();
    let (method, url, request_body) = match command {
        "exchange_plan" => {
            headers.insert(
                "content-type".into(),
                json!("application/x-www-form-urlencoded"),
            );
            let body = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("grant_type", "authorization_code")
                .append_pair("code", field(&metadata, "code")?)
                .append_pair("redirect_uri", CODEX_OAUTH_REDIRECT_URI)
                .append_pair("client_id", CODEX_OAUTH_CLIENT_ID)
                .append_pair("code_verifier", field(&metadata, "verifier")?)
                .finish();
            ("POST", endpoints.token_url()?, body.into_bytes())
        }
        "refresh_plan" => {
            headers.insert("content-type".into(), json!("application/json"));
            (
                "POST",
                endpoints.token_url()?,
                serde_json::to_vec(&json!({
                    "client_id":CODEX_OAUTH_CLIENT_ID,"grant_type":"refresh_token",
                    "refresh_token":field(&metadata,"refresh_token")?
                }))
                .map_err(|_| CodexConnectorError::InvalidTokenResponse)?,
            )
        }
        "models_plan" | "quota_plan" | "quota_reset_plan" => {
            let identity: CodexOutboundIdentity =
                serde_json::from_value(metadata["settings"].clone())
                    .map_err(|_| CodexConnectorError::InvalidCredential)?;
            let auth = codex_headers(
                &identity,
                field(&metadata, "access_token")?,
                metadata["account_id"].as_str(),
                metadata["is_fedramp"].as_bool().unwrap_or(false),
            )?;
            for (name, value) in auth.iter() {
                headers.insert(
                    name.to_string(),
                    json!(
                        value
                            .to_str()
                            .map_err(|_| CodexConnectorError::InvalidCredential)?
                    ),
                );
            }
            match command {
                "models_plan" => ("GET", endpoints.models_url(&identity)?, Vec::new()),
                "quota_plan" => ("GET", endpoints.quota_url()?, Vec::new()),
                _ => {
                    headers.insert("content-type".into(), json!("application/json"));
                    (
                        "POST",
                        endpoints.quota_reset_url()?,
                        serde_json::to_vec(&ConsumeQuotaResetCreditRequest {
                            redeem_request_id: field(&metadata, "redeem_request_id")?.into(),
                        })
                        .map_err(|_| CodexConnectorError::InvalidQuotaResponse)?,
                    )
                }
            }
        }
        _ => return Err(CodexConnectorError::InvalidCredential),
    };
    Ok(PluginOutput {
        metadata: json!({"method":method,"url":url,"headers":headers}),
        body: request_body,
    })
}
pub fn dispatch(
    command: &str,
    metadata: Value,
    body: &[u8],
) -> Result<PluginOutput, PluginCallError> {
    match process(command, metadata, body) {
        Ok(output) => Ok(PluginOutput {
            metadata: json!({"result": output.metadata}),
            body: output.body,
        }),
        Err(error) => Ok(PluginOutput {
            metadata: json!({"protocol_error":error}),
            body: Vec::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured_identity() -> CodexOutboundIdentity {
        CodexOutboundIdentity {
            originator: "codex_gateway".into(),
            client_version: "9.8.7".into(),
            user_agent: "codex_gateway/9.8.7 (Linux 6.8.0; x86_64) ai-gateway".into(),
        }
    }
    fn jwt(payload: Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{}");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.signature")
    }
    #[test]
    fn derives_codex_models_and_quota_endpoints_without_url_join_truncation() {
        let endpoints = CodexEndpoints::default();
        let identity = configured_identity();

        assert_eq!(
            endpoints.models_url(&identity).unwrap().path(),
            "/backend-api/codex/models"
        );
        assert_eq!(
            endpoints
                .models_url(&identity)
                .unwrap()
                .query_pairs()
                .find(|(name, _)| name == "client_version")
                .map(|(_, value)| value.into_owned()),
            Some(identity.client_version().to_owned())
        );
        assert_eq!(
            endpoints.quota_url().unwrap().path(),
            "/backend-api/wham/usage"
        );
        assert_eq!(
            endpoints.quota_reset_url().unwrap().path(),
            "/backend-api/wham/rate-limit-reset-credits/consume"
        );
    }
    #[test]
    fn builds_authorize_url_with_pkce_state_and_configured_originator() {
        let endpoints = CodexEndpoints::default();
        let identity = configured_identity();
        let url = Url::parse(
            &build_authorize_url(
                &endpoints,
                &PkceCodes {
                    challenge: "challenge".into(),
                },
                "state-value",
                &identity,
            )
            .unwrap(),
        )
        .unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(url.path(), "/oauth/authorize");
        assert_eq!(
            query.get("client_id").map(|value| value.as_ref()),
            Some(CODEX_OAUTH_CLIENT_ID)
        );
        assert_eq!(
            query.get("redirect_uri").map(|value| value.as_ref()),
            Some(CODEX_OAUTH_REDIRECT_URI)
        );
        assert_eq!(
            query.get("code_challenge").map(|value| value.as_ref()),
            Some("challenge")
        );
        assert_eq!(
            query
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        assert_eq!(
            query.get("state").map(|value| value.as_ref()),
            Some("state-value")
        );
        assert_eq!(
            query.get("originator").map(|value| value.as_ref()),
            Some(identity.originator())
        );
    }
    #[test]
    fn codex_headers_use_the_configured_identity() {
        let identity = configured_identity();
        let headers = codex_headers(&identity, "access-token", None, false).unwrap();
        assert_eq!(
            headers
                .get("originator")
                .and_then(|value| value.to_str().ok()),
            Some(identity.originator())
        );
        assert_eq!(
            headers.get("version").and_then(|value| value.to_str().ok()),
            Some(identity.client_version())
        );
        assert_eq!(
            headers
                .get(USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some(identity.user_agent())
        );
    }
    #[test]
    fn callback_parser_requires_the_exact_loopback_redirect_and_state() {
        let callback = parse_callback_url(
            "http://localhost:1455/auth/callback?code=code-value&state=state-value",
        )
        .unwrap();
        assert_eq!(callback.code, "code-value");
        assert_eq!(callback.state, "state-value");

        assert!(matches!(
            parse_callback_url(
                "https://localhost:1455/auth/callback?code=code-value&state=state-value"
            ),
            Err(CodexConnectorError::InvalidCallback)
        ));
        assert!(matches!(
            parse_callback_url(
                "http://localhost.evil.test:1455/auth/callback?code=code-value&state=state-value"
            ),
            Err(CodexConnectorError::InvalidCallback)
        ));
        assert!(matches!(
            parse_callback_url(
                "http://user@localhost:1455/auth/callback?code=code-value&state=state-value"
            ),
            Err(CodexConnectorError::InvalidCallback)
        ));
        assert!(matches!(
            parse_callback_url(
                "http://localhost:1455/auth/callback?code=code-value&state=state-value#fragment"
            ),
            Err(CodexConnectorError::InvalidCallback)
        ));
        assert!(matches!(
            parse_callback_url(
                "http://localhost:1455/auth/callback?error=access_denied&state=state-value"
            ),
            Err(CodexConnectorError::OauthDenied)
        ));
    }
    #[test]
    fn parses_codex_identity_and_access_token_expiration_without_exposing_tokens() {
        let identity = parse_identity(&jwt(json!({
            "https://api.openai.com/profile": {"email": "codex@example.test"},
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "account-123",
                "chatgpt_user_id": "user-456",
                "chatgpt_plan_type": "plus",
                "chatgpt_account_is_fedramp": true
            }
        })))
        .unwrap();
        assert_eq!(identity.email.as_deref(), Some("codex@example.test"));
        assert_eq!(identity.account_id.as_deref(), Some("account-123"));
        assert_eq!(identity.user_id.as_deref(), Some("user-456"));
        assert_eq!(identity.plan_type.as_deref(), Some("plus"));
        assert!(identity.is_fedramp);

        let fallback_identity = parse_identity(&jwt(json!({
            "sub": "subject-user",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "business-workspace",
                "user_id": "fallback-user"
            }
        })))
        .unwrap();
        assert_eq!(fallback_identity.user_id.as_deref(), Some("fallback-user"));

        let personal_identity = parse_identity(&jwt(json!({
            "email": "free@example.test",
            "sub": "personal-user",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "free"
            }
        })))
        .unwrap();
        assert_eq!(personal_identity.account_id, None);
        assert_eq!(personal_identity.user_id.as_deref(), Some("personal-user"));

        let headers = codex_headers(&configured_identity(), "access-token", None, false).unwrap();
        assert!(!headers.contains_key("chatgpt-account-id"));

        let expires_at = parse_jwt_expiration(&jwt(json!({"exp": 1_800_000_000})))
            .unwrap()
            .unwrap();
        assert_eq!(expires_at.timestamp(), 1_800_000_000);
        assert!(matches!(
            parse_identity(&format!("{}.extra", jwt(json!({})))),
            Err(CodexConnectorError::InvalidJwt)
        ));
        assert!(matches!(
            parse_jwt_expiration(&jwt(json!({"exp": i64::MAX}))),
            Err(CodexConnectorError::InvalidJwt)
        ));
    }
    #[test]
    fn quota_windows_reject_malformed_usage_snapshots() {
        assert!(matches!(
            validate_window(Some(QuotaWindow {
                used_percent: 101,
                limit_window_seconds: 10_800,
                reset_at: 1_800_000_000,
            })),
            Err(CodexConnectorError::InvalidQuotaResponse)
        ));
        assert!(matches!(
            validate_window(Some(QuotaWindow {
                used_percent: 50,
                limit_window_seconds: 0,
                reset_at: 1_800_000_000,
            })),
            Err(CodexConnectorError::InvalidQuotaResponse)
        ));
    }
    #[test]
    fn refresh_token_reuse_is_classified_as_a_permanent_credential_failure() {
        let error = token_endpoint_error(
            StatusCode::BAD_REQUEST,
            br#"{"error":{"code":"refresh_token_reused"}}"#,
            true,
        );
        assert!(matches!(error, CodexConnectorError::RefreshTokenInvalid));
    }

    #[test]
    fn provider_response_parsers_preserve_models_quota_and_reset_semantics() {
        let models = parse_models(
            StatusCode::OK,
            br#"{"models":[{"slug":"a"},{"slug":"a"},{"slug":"hidden","supported_in_api":false}]}"#,
        )
        .unwrap();
        assert_eq!(models, vec!["a"]);
        let quota=parse_quota(StatusCode::OK,br#"{"rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":42,"limit_window_seconds":10800,"reset_at":1800000000}},"rate_limit_reset_credits":{"available_count":2}}"#).unwrap();
        assert_eq!(quota.primary_used_percent, Some(42));
        assert_eq!(quota.primary_reset_at.unwrap().timestamp(), 1800000000);
        assert_eq!(quota.reset_credits_available, Some(2));
        let reset =
            parse_quota_reset(StatusCode::OK, br#"{"code":"reset","windows_reset":2}"#).unwrap();
        assert_eq!(reset.outcome, CodexQuotaResetOutcome::Reset);
        assert_eq!(reset.windows_reset, 2);
        assert!(
            parse_quota_reset(StatusCode::OK, br#"{"code":"reset","windows_reset":3}"#).is_err()
        );
        assert!(parse_quota(StatusCode::OK, br#"{"rate_limit":null}"#).is_err());
        assert!(parse_quota(StatusCode::OK,br#"{"rate_limit":{"allowed":true,"limit_reached":false},"rate_limit_reset_credits":{"available_count":-1}}"#).is_err());
    }
    #[test]
    fn token_response_validation_and_refresh_failure_classification() {
        assert!(
            parse_exchange(
                StatusCode::OK,
                br#"{"id_token":"","access_token":"a","refresh_token":"r"}"#
            )
            .is_err()
        );
        assert!(parse_refresh(StatusCode::OK, b"{}").is_err());
        let refreshed =
            parse_refresh(StatusCode::OK, br#"{"refresh_token":"replacement"}"#).unwrap();
        assert_eq!(refreshed.refresh_token.as_deref(), Some("replacement"));
        assert!(matches!(
            parse_refresh(StatusCode::UNAUTHORIZED, b"secret"),
            Err(CodexConnectorError::RefreshTokenInvalid)
        ));
        assert!(matches!(
            parse_refresh(StatusCode::INTERNAL_SERVER_ERROR, b"secret"),
            Err(CodexConnectorError::TokenEndpointStatus(500))
        ));
    }
    #[test]
    fn dispatch_uses_separate_plan_and_parse_operations() {
        let endpoints = dispatch("endpoints", json!({}), &[]).unwrap().metadata["result"].clone();
        let plan = dispatch(
            "exchange_plan",
            json!({"endpoints":endpoints,"code":"a&b","verifier":"verify"}),
            &[],
        )
        .unwrap();
        assert_eq!(
            plan.metadata["result"]["url"],
            "https://auth.openai.com/oauth/token"
        );
        assert_eq!(plan.metadata["result"]["method"], "POST");
        let body = String::from_utf8(plan.body).unwrap();
        assert!(body.contains("code=a%26b"));
        let parsed = dispatch(
            "refresh_parse",
            json!({"status":400}),
            br#"{"error":{"code":"refresh_token_reused"}}"#,
        )
        .unwrap();
        assert_eq!(parsed.metadata["protocol_error"], "RefreshTokenInvalid");
    }
}
