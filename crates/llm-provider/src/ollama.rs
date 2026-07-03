use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::LlmProvider;
use crate::request::{ChatMessage, LlmRequest, LlmResponse};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Default base URL for a locally running Ollama server.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: ChatMessage,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    name: String,
}

#[derive(Debug, Serialize)]
struct OllamaPullRequest {
    model: String,
    stream: bool,
}

/// A single progress line from Ollama's streaming `/api/pull` response.
///
/// Ollama emits one JSON object per line (NDJSON): manifest/verify status
/// lines carry only `status`, download lines add `digest`/`total`/`completed`,
/// and a terminal `status: "success"` line signals completion. A failure
/// mid-stream (e.g. disk full) can arrive as a line with `error` set instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullProgress {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

impl PullProgress {
    /// Whether this progress line signals the pull has finished successfully.
    pub fn is_done(&self) -> bool {
        self.status == "success"
    }
}

/// An [`LlmProvider`] backed by a local (or remote) Ollama server, using
/// Ollama's native `/api/chat`, `/api/tags`, and `/api/ps` endpoints rather
/// than its OpenAI-compatible shim, plus a streaming `pull` for downloading
/// models that isn't part of the shared trait.
pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    /// Construct a provider pointed at `base_url` (trailing slash tolerated).
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    /// The base URL this provider talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Pull (download) a model, streaming progress events as Ollama reports
    /// them. The returned receiver yields one [`PullProgress`] per NDJSON
    /// line; the channel closes once the stream ends (successfully or not).
    /// Cancelling `cancel` stops the request before it starts, or aborts the
    /// background stream reader mid-pull.
    pub async fn pull(
        &self,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<mpsc::UnboundedReceiver<PullProgress>> {
        if cancel.is_cancelled() {
            bail!("pull was cancelled before it started");
        }

        let url = format!("{}/api/pull", self.base_url);
        let body = OllamaPullRequest {
            model: model.to_string(),
            stream: true,
        };

        let send_future = self.client.post(&url).json(&body).send();

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                bail!("pull was cancelled");
            }
            result = send_future => {
                result.context("failed to send pull request to Ollama")?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!(
                "Ollama pull endpoint returned HTTP {}: {}",
                status.as_u16(),
                text
            );
        }

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(stream_pull_progress(response, tx, cancel));

        Ok(rx)
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

/// Read `response`'s body as it arrives, splitting on newlines and parsing
/// each complete NDJSON line into a [`PullProgress`] forwarded on `tx`.
async fn stream_pull_progress(
    response: reqwest::Response,
    tx: mpsc::UnboundedSender<PullProgress>,
    cancel: CancellationToken,
) {
    let mut stream = response.bytes_stream();
    let mut buf = String::new();

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return,
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(idx) = buf.find('\n') {
                            let line = buf[..idx].trim().to_string();
                            buf.drain(..=idx);
                            if !emit_line(&line, &tx) {
                                return;
                            }
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    let trailing = buf.trim().to_string();
    if !trailing.is_empty() {
        emit_line(&trailing, &tx);
    }
}

/// Parse `line` as a [`PullProgress`] and send it. Returns `false` if the
/// receiver has hung up (no point continuing) or the pull is done.
fn emit_line(line: &str, tx: &mpsc::UnboundedSender<PullProgress>) -> bool {
    if line.is_empty() {
        return true;
    }
    let Ok(progress) = serde_json::from_str::<PullProgress>(line) else {
        return true;
    };
    let done = progress.is_done();
    if tx.send(progress).is_err() {
        return false;
    }
    !done
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(&self, request: &LlmRequest, cancel: CancellationToken) -> Result<LlmResponse> {
        if cancel.is_cancelled() {
            bail!("request was cancelled before it started");
        }

        let url = format!("{}/api/chat", self.base_url);
        let body = OllamaChatRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            stream: false,
            options: OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            },
        };

        let send_future = self.client.post(&url).json(&body).send();

        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                bail!("request was cancelled");
            }
            result = send_future => {
                result.context("failed to send request to Ollama")?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("Ollama returned HTTP {}: {}", status.as_u16(), text);
        }

        let parsed: OllamaChatResponse = response
            .json()
            .await
            .context("failed to parse Ollama chat response")?;

        Ok(LlmResponse {
            text: parsed.message.content,
            model: parsed.model,
            usage_tokens: parsed.eval_count,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("failed to reach Ollama tags endpoint")?;

        if !response.status().is_success() {
            bail!("tags endpoint returned HTTP {}", response.status().as_u16());
        }

        let parsed: OllamaTagsResponse = response
            .json()
            .await
            .context("failed to parse Ollama tags response")?;

        Ok(parsed.models.into_iter().map(|m| m.name).collect())
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/ps", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn provider_name(&self) -> &'static str {
        "ollama"
    }
}
