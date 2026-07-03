use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::provider::LlmProvider;
use crate::request::{ChatMessage, LlmRequest, LlmResponse};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

#[derive(Debug, Serialize)]
struct GeminiChatRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GeminiChatResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "totalTokenCount")]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Debug, Deserialize)]
struct GeminiModel {
    name: String,
}

/// Maps a Gemini role. Gemini uses "model" instead of OpenAI's "assistant"
/// for the responder role; everything else (notably "user") passes through.
fn to_gemini_role(role: &str) -> String {
    match role {
        "assistant" => "model".to_string(),
        other => other.to_string(),
    }
}

/// Splits the request's messages into an optional Gemini `systemInstruction`
/// (concatenation of any `system`-role messages) plus the remaining
/// non-system messages mapped into Gemini's `{role, parts}` content shape.
fn split_system_and_contents(
    messages: &[ChatMessage],
) -> (Option<GeminiContent>, Vec<GeminiContent>) {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();

    for message in messages {
        if message.role == "system" {
            system_parts.push(message.content.clone());
        } else {
            contents.push(GeminiContent {
                role: Some(to_gemini_role(&message.role)),
                parts: vec![GeminiPart {
                    text: message.content.clone(),
                }],
            });
        }
    }

    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(GeminiContent {
            role: None,
            parts: vec![GeminiPart {
                text: system_parts.join("\n\n"),
            }],
        })
    };

    (system_instruction, contents)
}

/// An LLM provider that speaks Google's Gemini `generateContent` API
/// (`POST /v1beta/models/{model}:generateContent?key=…`,
/// `GET /v1beta/models?key=…`), passing the API key as a query parameter
/// and mapping system messages to a top-level `systemInstruction`.
pub struct GeminiProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl GeminiProvider {
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

    /// Construct a provider pointed at the production Gemini API.
    pub fn with_default_base(api_key: &str) -> Self {
        Self::new(DEFAULT_BASE_URL, api_key)
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn chat(&self, request: &LlmRequest, cancel: CancellationToken) -> Result<LlmResponse> {
        if cancel.is_cancelled() {
            bail!("request was cancelled before it started");
        }

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, request.model
        );
        let (system_instruction, contents) = split_system_and_contents(&request.messages);
        let body = GeminiChatRequest {
            contents,
            system_instruction,
            generation_config: GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            },
        };

        let send_future = self
            .client
            .post(&url)
            .query(&[("key", self.api_key.as_str())])
            .json(&body)
            .send();

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                bail!("request was cancelled");
            }
            result = send_future => {
                result.context("failed to send request to Gemini endpoint")?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "Gemini endpoint returned HTTP {}: {}",
                status.as_u16(),
                body
            );
        }

        let parsed: GeminiChatResponse = response
            .json()
            .await
            .context("failed to parse Gemini chat response")?;

        // If `candidates` is empty (e.g. the response was blocked by safety
        // filters), this intentionally yields an empty string rather than
        // erroring — callers see an empty completion, not a failure.
        let text = parsed
            .candidates
            .into_iter()
            .next()
            .map(|c| {
                c.content
                    .parts
                    .into_iter()
                    .map(|p| p.text)
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            model: request.model.clone(),
            usage_tokens: parsed.usage_metadata.map(|u| u.total_token_count),
        })
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1beta/models", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("key", self.api_key.as_str())])
            .send()
            .await
            .context("failed to reach models endpoint")?;

        if !response.status().is_success() {
            bail!(
                "models endpoint returned HTTP {}",
                response.status().as_u16()
            );
        }

        let parsed: GeminiModelsResponse = response
            .json()
            .await
            .context("failed to parse models response")?;

        Ok(parsed
            .models
            .into_iter()
            .map(|m| {
                m.name
                    .strip_prefix("models/")
                    .map(|s| s.to_string())
                    .unwrap_or(m.name)
            })
            .collect())
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1beta/models", self.base_url);
        self.client
            .get(&url)
            .query(&[("key", self.api_key.as_str())])
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn provider_name(&self) -> &'static str {
        "gemini"
    }
}
