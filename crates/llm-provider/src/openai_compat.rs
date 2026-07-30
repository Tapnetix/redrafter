use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::provider::LlmProvider;
use crate::request::{ChatMessage, LlmRequest, LlmResponse};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    model: String,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}

/// An LLM provider that speaks the OpenAI chat-completions API shape
/// against an arbitrary base URL. Covers OpenAI itself, Alibaba's
/// DashScope-compatible endpoint, and self-hosted servers (e.g. Ollama,
/// vLLM, llama.cpp server) that implement the same `/v1/chat/completions`
/// and `/v1/models` routes.
pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiCompatProvider {
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
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat(&self, request: &LlmRequest, cancel: CancellationToken) -> Result<LlmResponse> {
        if cancel.is_cancelled() {
            bail!("request was cancelled before it started");
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = OpenAiChatRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let send_future = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send();

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                bail!("request was cancelled");
            }
            result = send_future => {
                result.context("failed to send request to OpenAI-compatible endpoint")?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "OpenAI-compatible endpoint returned HTTP {}: {}",
                status.as_u16(),
                body
            );
        }

        let parsed: OpenAiChatResponse = response
            .json()
            .await
            .context("failed to parse OpenAI-compatible chat response")?;

        let text = parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            model: parsed.model,
            usage_tokens: parsed.usage.map(|u| u.total_tokens),
        })
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .context("failed to reach models endpoint")?;

        if !response.status().is_success() {
            bail!(
                "models endpoint returned HTTP {}",
                response.status().as_u16()
            );
        }

        let parsed: OpenAiModelsResponse = response
            .json()
            .await
            .context("failed to parse models response")?;

        Ok(parsed.data.into_iter().map(|m| m.id).collect())
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        self.client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn provider_name(&self) -> &'static str {
        "openai-compatible"
    }
}
