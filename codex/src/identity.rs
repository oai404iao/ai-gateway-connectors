//! Credential-stable installation identity and per-attempt caller identity.

use ai_gateway_connector_sdk::PluginCallError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{request_policy::CodexRequestMetadata, settings::Settings};

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RequestIdentity {
    installation_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub window_id: String,
}

impl RequestIdentity {
    pub fn prepare(m: &Value) -> Result<Self, PluginCallError> {
        let invalid = || PluginCallError::new("invalid_request_context", "Invalid request context");
        let credential = m["credential_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(invalid)?;
        let request_id = m["request_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(invalid)?;
        let header = |name| {
            m["headers"][name]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 512)
                .map(str::to_owned)
        };
        let mut seed = [0; 32];
        seed[..16].copy_from_slice(request_id.as_bytes());
        seed[16..].copy_from_slice(request_id.as_bytes());
        if !matches!(
            m["operation"].as_str(),
            Some("images_generation" | "images_edit")
        ) && !m["affinity_hash"].is_null()
        {
            seed = serde_json::from_value(m["affinity_hash"].clone()).map_err(|_| invalid())?;
        }
        let (session_id, thread_id) = match (header("session-id"), header("thread-id")) {
            (Some(s), Some(t)) => (s, t),
            (Some(s), None) => (s.clone(), s),
            (None, Some(t)) => (t.clone(), t),
            (None, None) => (
                opaque_uuid(&seed, b"codex-session"),
                opaque_uuid(&seed, b"codex-thread"),
            ),
        };
        let window_id = header("x-codex-window-id").unwrap_or_else(|| format!("{thread_id}:0"));
        Ok(Self {
            installation_id: opaque_uuid(credential.as_bytes(), b"ai-gateway-codex-installation"),
            session_id,
            thread_id,
            window_id,
            turn_id: request_id.to_string(),
        })
    }

    pub fn from_metadata(m: &Value) -> Result<Self, PluginCallError> {
        serde_json::from_value(m["request_context"].clone())
            .map_err(|_| PluginCallError::new("invalid_request_context", "Missing request context"))
    }

    pub fn privacy(&self, settings: &Settings) -> CodexRequestMetadata {
        CodexRequestMetadata::new(
            self.installation_id.clone(),
            self.session_id.clone(),
            self.thread_id.clone(),
            self.turn_id.clone(),
            self.window_id.clone(),
            settings.workspace_path.clone(),
            settings.git_remote_url.clone(),
        )
    }
}

fn opaque_uuid(seed: &[u8], domain: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(seed);
    let digest = hasher.finalize();
    let mut value = [0; 16];
    value.copy_from_slice(&digest[..16]);
    value[6] = (value[6] & 0x0f) | 0x40;
    value[8] = (value[8] & 0x3f) | 0x80;
    Uuid::from_bytes(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credential_identity_is_stable_across_requests_and_operations() {
        let mut m = json!({"credential_id":"11111111-1111-4111-8111-111111111111",
            "request_id":"22222222-2222-4222-8222-222222222222","headers":{}});
        m["affinity_hash"] = json!(vec![1u8; 32]);
        let a = RequestIdentity::prepare(&m).unwrap();
        assert_eq!(a.installation_id, "d3c8659c-e5a3-42ed-aa68-abd014393d91");
        m["request_id"] = json!("33333333-3333-4333-8333-333333333333");
        let b = RequestIdentity::prepare(&m).unwrap();
        assert_eq!(a.installation_id, b.installation_id);
        assert_eq!(a.session_id, b.session_id);
        assert_ne!(a.turn_id, b.turn_id);
        m["operation"] = json!("images_edit");
        assert_eq!(
            a.installation_id,
            RequestIdentity::prepare(&m).unwrap().installation_id
        );
        m["credential_id"] = json!("44444444-4444-4444-8444-444444444444");
        assert_ne!(
            a.installation_id,
            RequestIdentity::prepare(&m).unwrap().installation_id
        );
    }
}
