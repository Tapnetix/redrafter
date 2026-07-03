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
    temperature: f32,
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

/// An LLM provider that speaks Anthropic's Messages API
/// (`POST /v1/messages`, `GET /v1/models`), using `x-api-key` +
/// `anthropic-version` headers and a top-level `system` prompt rather than
/// an OpenAI-style `system` message in the `messages` array.
pub struct AnthropicProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
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
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
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
            bail!(
                "Anthropic endpoint returned HTTP {}: {}",
                status.as_u16(),
                body
            );
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
            .client
            .get(&url)
            .header("x-api-key", &self.api_key)
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
        let url = format!("{}/v1/models", self.base_url);
        self.client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn provider_name(&self) -> &'static str {
        "anthropic"
    }
}
