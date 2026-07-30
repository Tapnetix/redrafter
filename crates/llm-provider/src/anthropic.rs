use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::provider::LlmProvider;
use crate::request::{ChatMessage, LlmRequest, LlmResponse};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Serialize)]
struct AnthropicChatRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicChatResponse {
    content: Vec<AnthropicContentBlock>,
    model: String,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    block_type: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
}

/// How a request authenticates to the Anthropic API.
///
/// Console API keys go in `x-api-key`; the OAuth access tokens Claude Code
/// stores (`sk-ant-oat…`, carrying a `user:inference` scope) go in
/// `Authorization: Bearer`. Sending an OAuth token as `x-api-key` is rejected
/// with `invalid x-api-key`, so the two are not interchangeable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicAuth {
    /// A Console API key (`sk-ant-api…`), billed to an organization.
    ApiKey(String),
    /// An OAuth access token, billed against the account's subscription.
    OAuth(String),
}

impl AnthropicAuth {
    /// The header name/value this credential authenticates with.
    pub fn header(&self) -> (&'static str, &str) {
        match self {
            AnthropicAuth::ApiKey(key) => ("x-api-key", key.as_str()),
            AnthropicAuth::OAuth(token) => ("authorization", token.as_str()),
        }
    }

    /// The value to send, pre-formatted (Bearer prefix for OAuth).
    pub fn header_value(&self) -> String {
        match self {
            AnthropicAuth::ApiKey(key) => key.clone(),
            AnthropicAuth::OAuth(token) => format!("Bearer {token}"),
        }
    }
}

/// An LLM provider that speaks Anthropic's Messages API
/// (`POST /v1/messages`, `GET /v1/models`), with `anthropic-version` and a
/// top-level `system` prompt rather than an OpenAI-style `system` message in
/// the `messages` array. Authenticates with either a Console API key or an
/// OAuth access token — see [`AnthropicAuth`].
pub struct AnthropicProvider {
    base_url: String,
    auth: AnthropicAuth,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        // Trimmed because keys are pasted: a trailing newline or space makes
        // reqwest refuse to build the header at all, and the resulting send
        // error was indistinguishable from "the server is down".
        Self::with_auth(base_url, AnthropicAuth::ApiKey(api_key.trim().to_string()))
    }

    /// Builds a provider authenticating with `auth`.
    pub fn with_auth(base_url: &str, auth: AnthropicAuth) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            auth,
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    /// Construct a provider pointed at the production Anthropic API.
    pub fn with_default_base(api_key: &str) -> Self {
        Self::new(DEFAULT_BASE_URL, api_key)
    }

    /// Applies the configured credential to a request.
    fn authenticate(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let (name, _) = self.auth.header();
        request.header(name, self.auth.header_value())
    }
}

/// A targeted hint when the credential is the *wrong kind*, rather than merely
/// wrong.
///
/// Anthropic issues several `sk-ant-…` credentials that are not
/// interchangeable, and the API's own 401 ("invalid x-api-key") can't tell you
/// which mistake you made. The common one: a token minted by Claude Code is an
/// OAuth token authenticating as `Authorization: Bearer`, so pasting it into
/// an API-key field always fails no matter how many times it's regenerated.
fn credential_hint(auth: &AnthropicAuth) -> Option<&'static str> {
    let AnthropicAuth::ApiKey(key) = auth else {
        return None;
    };
    if key.starts_with("sk-ant-oat") {
        Some(
            "that looks like a Claude Code OAuth token rather than a Console API key — \
             use the \"Use your Claude Code login\" button instead of pasting it as a key",
        )
    } else if key.starts_with("sk-ant-ort") {
        Some("that is an OAuth *refresh* token, not an API key")
    } else if key.starts_with("sk-ant-sid") {
        Some("that looks like a claude.ai session key rather than a Console API key")
    } else if key.starts_with("sk-ant-admin") {
        Some("that is an Admin API key, which cannot call the Messages API")
    } else if !key.starts_with("sk-ant-") {
        Some("Console API keys start with \"sk-ant-\"")
    } else {
        None
    }
}

/// Pulls `error.message` out of an Anthropic error body, if it looks like one.
/// Returns `None` for anything unparseable so the caller can fall back to the
/// bare status rather than printing a wall of HTML.
fn extract_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = value.get("error")?.get("message")?.as_str()?.trim();
    if message.is_empty() {
        return None;
    }
    Some(message.to_string())
}

/// Splits the request's messages into an Anthropic top-level `system`
/// prompt (concatenation of any `system`-role messages) plus the remaining
/// non-system messages in Anthropic's `{role, content}` shape.
fn split_system_and_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system_parts = Vec::new();
    let mut rest = Vec::new();

    for message in messages {
        if message.role == "system" {
            system_parts.push(message.content.clone());
        } else {
            rest.push(AnthropicMessage {
                role: message.role.clone(),
                content: message.content.clone(),
            });
        }
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    (system, rest)
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, request: &LlmRequest, cancel: CancellationToken) -> Result<LlmResponse> {
        if cancel.is_cancelled() {
            bail!("request was cancelled before it started");
        }

        let url = format!("{}/v1/messages", self.base_url);
        let (system, messages) = split_system_and_messages(&request.messages);
        let body = AnthropicChatRequest {
            model: request.model.clone(),
            messages,
            system,
            temperature: request.temperature,
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        };

        let send_future = self
            .authenticate(self.client.post(&url))
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send();

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                bail!("request was cancelled");
            }
            result = send_future => {
                result.context("failed to send request to Anthropic endpoint")?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            // Report the API's own message, not the raw envelope. The whole
            // body ({"type":"error","error":{...},"request_id":...}) is far
            // too long for the failure chip, so the user saw the first couple
            // of words and nothing that told them what to change.
            let mut reason = match extract_error_message(&body) {
                Some(message) => format!("HTTP {}: {message}", status.as_u16()),
                None => format!("HTTP {}: {body}", status.as_u16()),
            };
            if status == reqwest::StatusCode::UNAUTHORIZED {
                if let Some(hint) = credential_hint(&self.auth) {
                    reason.push_str(" — ");
                    reason.push_str(hint);
                }
            }
            bail!("Anthropic {reason}");
        }

        let parsed: AnthropicChatResponse = response
            .json()
            .await
            .context("failed to parse Anthropic chat response")?;

        let text = parsed
            .content
            .into_iter()
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse {
            text,
            model: parsed.model,
            usage_tokens: parsed.usage.map(|u| u.input_tokens + u.output_tokens),
        })
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self
            .authenticate(self.client.get(&url))
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("failed to reach models endpoint")?;

        if !response.status().is_success() {
            bail!(
                "models endpoint returned HTTP {}",
                response.status().as_u16()
            );
        }

        let parsed: AnthropicModelsResponse = response
            .json()
            .await
            .context("failed to parse models response")?;

        Ok(parsed.data.into_iter().map(|m| m.id).collect())
    }

    async fn is_available(&self) -> bool {
        self.availability().await.is_ok()
    }

    async fn availability(&self) -> std::result::Result<(), String> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self
            .authenticate(self.client.get(&url))
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .map_err(|e| format!("could not reach {url}: {e}"))?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        // Anthropic replies with `{"error":{"message":"…"}}`; surface that
        // message ("API key is invalid.", "credit balance is too low", …)
        // rather than the bare status, since it is the actionable half.
        let body = response.text().await.unwrap_or_default();
        let mut reason = match extract_error_message(&body) {
            Some(message) => format!("{status}: {message}"),
            None => format!("{status} from {url}"),
        };
        // Only on an auth failure: a hint about the credential's shape is
        // noise next to a 429 or a 500.
        if status == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(hint) = credential_hint(&self.auth) {
                reason.push_str(" — ");
                reason.push_str(hint);
            }
        }
        Err(reason)
    }

    fn provider_name(&self) -> &'static str {
        "anthropic"
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn a_console_api_key_goes_in_x_api_key_unprefixed() {
        let auth = AnthropicAuth::ApiKey("sk-ant-api-xyz".to_string());
        assert_eq!(auth.header().0, "x-api-key");
        assert_eq!(auth.header_value(), "sk-ant-api-xyz");
    }

    #[test]
    fn an_oauth_token_goes_in_authorization_with_a_bearer_prefix() {
        // Verified against the live API: an OAuth token sent as `x-api-key`
        // is rejected with `invalid x-api-key` (401), while the same token as
        // `Authorization: Bearer` authenticates.
        let auth = AnthropicAuth::OAuth("sk-ant-oat-xyz".to_string());
        assert_eq!(auth.header().0, "authorization");
        assert_eq!(auth.header_value(), "Bearer sk-ant-oat-xyz");
    }

    #[test]
    fn the_two_credential_kinds_never_share_a_header() {
        let key = AnthropicAuth::ApiKey("k".to_string());
        let oauth = AnthropicAuth::OAuth("k".to_string());
        assert_ne!(key.header().0, oauth.header().0);
        assert_ne!(key.header_value(), oauth.header_value());
    }

    #[test]
    fn new_defaults_to_api_key_auth_so_existing_callers_are_unchanged() {
        let provider = AnthropicProvider::new("https://api.anthropic.com/", "sk-ant-api-1");
        assert_eq!(provider.auth, AnthropicAuth::ApiKey("sk-ant-api-1".to_string()));
        assert_eq!(provider.base_url, "https://api.anthropic.com");
    }
}

#[cfg(test)]
mod availability_tests {
    use super::*;

    #[test]
    fn extracts_the_actionable_message_from_an_anthropic_error_body() {
        // The exact body the live API returns for a bad key.
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"API key is invalid."},"request_id":null}"#;
        assert_eq!(
            extract_error_message(body).as_deref(),
            Some("API key is invalid.")
        );
    }

    #[test]
    fn extracts_the_missing_version_header_message() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"anthropic-version: header is required"}}"#;
        assert_eq!(
            extract_error_message(body).as_deref(),
            Some("anthropic-version: header is required")
        );
    }

    #[test]
    fn falls_back_to_none_for_a_non_json_body() {
        // A proxy or captive portal returning HTML must not be pasted into
        // the UI wholesale.
        assert_eq!(extract_error_message("<html><body>502</body></html>"), None);
        assert_eq!(extract_error_message(""), None);
    }

    #[test]
    fn falls_back_to_none_when_the_shape_is_unexpected() {
        assert_eq!(extract_error_message(r#"{"error":"a string"}"#), None);
        assert_eq!(extract_error_message(r#"{"error":{"message":"   "}}"#), None);
        assert_eq!(extract_error_message(r#"{"detail":"nope"}"#), None);
    }

    #[test]
    fn a_pasted_key_is_trimmed_so_the_header_can_be_built() {
        // A trailing newline from a paste makes reqwest refuse to build the
        // header, which surfaced as an unexplained "could not connect".
        let provider = AnthropicProvider::new("https://api.anthropic.com", "  sk-ant-api-1\n");
        assert_eq!(
            provider.auth,
            AnthropicAuth::ApiKey("sk-ant-api-1".to_string())
        );
    }

    #[test]
    fn a_pasted_base_url_is_trimmed_of_space_and_trailing_slashes() {
        let provider = AnthropicProvider::new("  https://api.anthropic.com/  ", "k");
        assert_eq!(provider.base_url, "https://api.anthropic.com");
    }
}

#[cfg(test)]
mod credential_hint_tests {
    use super::*;

    fn hint(key: &str) -> Option<&'static str> {
        credential_hint(&AnthropicAuth::ApiKey(key.to_string()))
    }

    #[test]
    fn a_claude_code_oauth_token_pasted_as_a_key_is_named_as_such() {
        // The exact confusion this exists for: the API only says "invalid
        // x-api-key", so regenerating the token forever never helps.
        let h = hint("sk-ant-oat01-abcdef").expect("should be recognised");
        assert!(h.contains("Claude Code"), "got: {h}");
        assert!(h.contains("login"), "should point at the button; got: {h}");
    }

    #[test]
    fn other_sk_ant_credentials_are_told_apart() {
        assert!(hint("sk-ant-ort01-x").unwrap().contains("refresh"));
        assert!(hint("sk-ant-sid01-x").unwrap().contains("session key"));
        assert!(hint("sk-ant-admin01-x").unwrap().contains("Admin"));
    }

    #[test]
    fn something_that_is_not_an_anthropic_credential_at_all_says_so() {
        assert!(hint("hunter2").unwrap().contains("sk-ant-"));
        assert!(hint("sk-proj-openai-style").unwrap().contains("sk-ant-"));
    }

    #[test]
    fn a_real_console_api_key_gets_no_hint() {
        // A genuine key that is merely revoked/expired must not be second-
        // guessed — the API's own message is the useful one there.
        assert_eq!(hint("sk-ant-api03-realkey"), None);
    }

    #[test]
    fn an_oauth_credential_is_never_second_guessed() {
        // It authenticates as Bearer, so the api-key advice would be wrong.
        assert_eq!(
            credential_hint(&AnthropicAuth::OAuth("sk-ant-oat01-x".to_string())),
            None
        );
    }
}

#[cfg(test)]
mod temperature_tests {
    use super::*;
    use crate::request::{ChatMessage, LlmRequest};

    fn request(temperature: Option<f32>) -> LlmRequest {
        LlmRequest {
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            model: "claude-sonnet-5".into(),
            temperature,
            max_tokens: Some(64),
        }
    }

    fn body_json(request: &LlmRequest) -> serde_json::Value {
        let (system, messages) = split_system_and_messages(&request.messages);
        serde_json::to_value(AnthropicChatRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            temperature: request.temperature,
            system,
            messages,
        })
        .unwrap()
    }

    #[test]
    fn temperature_is_omitted_entirely_when_unset() {
        // Anthropic rejects the whole request with 400 "`temperature` is
        // deprecated for this model." on its newer models, so an unconditional
        // default made every refine on them fail.
        let json = body_json(&request(None));
        assert!(
            json.get("temperature").is_none(),
            "temperature must not appear at all: {json}"
        );
    }

    #[test]
    fn an_explicit_temperature_is_still_sent() {
        let json = body_json(&request(Some(0.3)));
        let sent = json["temperature"].as_f64().expect("temperature should be present");
        // f32 -> JSON widens to 0.30000001192092896; compare with tolerance.
        assert!((sent - 0.3).abs() < 1e-6, "got {sent}");
    }

    #[test]
    fn the_rest_of_the_body_is_unaffected() {
        let json = body_json(&request(None));
        assert_eq!(json["model"], "claude-sonnet-5");
        assert_eq!(json["max_tokens"], 64);
        assert_eq!(json["messages"][0]["content"], "hi");
    }
}
