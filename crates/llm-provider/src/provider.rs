use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::request::{LlmRequest, LlmResponse};

/// A chat-completion capable LLM backend.
///
/// Implementations are expected to be cheap to clone/share (`Send + Sync`)
/// so they can be held behind an `Arc` and reused across requests. `chat`
/// accepts a [`CancellationToken`] so a slow in-flight request can be
/// aborted mid-flight (e.g. the user cancels a refine operation).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request, honoring cancellation.
    async fn chat(&self, request: &LlmRequest, cancel: CancellationToken) -> Result<LlmResponse>;

    /// List the model identifiers available from this provider.
    async fn list_models(&self) -> Result<Vec<String>>;

    /// Cheaply check whether the provider is reachable/configured.
    async fn is_available(&self) -> bool;

    /// A short, stable identifier for this provider (e.g. "openai-compatible").
    fn provider_name(&self) -> &'static str;
}
