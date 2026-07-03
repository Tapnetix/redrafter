//! Model discovery layer.
//!
//! Given any [`LlmProvider`], attempts to list its available models via
//! `list_models()`. Not every provider exposes a working list endpoint, so
//! discovery must degrade gracefully to a manual-entry state rather than
//! erroring out -- the UI (see B14) can then prompt the user for a
//! free-text model id.

use crate::provider::LlmProvider;

/// Outcome of attempting to discover the models available from a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryResult {
    /// The provider successfully listed its available models.
    Discovered(Vec<String>),
    /// The provider does not support listing models, or listing failed;
    /// the caller should fall back to a manually-entered model id.
    ManualEntryRequired {
        /// Human-readable reason listing degraded (unsupported endpoint,
        /// network error, empty response, etc.), useful for surfacing in
        /// the UI/logs.
        reason: String,
    },
}

impl DiscoveryResult {
    /// Whether this result requires the caller to prompt for a manually
    /// entered model id (listing was unsupported, failed, or empty).
    pub fn is_manual(&self) -> bool {
        matches!(self, Self::ManualEntryRequired { .. })
    }
}

/// Error returned when a manually-entered model id fails the lightweight
/// local acceptance check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualModelIdError {
    /// The id was empty (or all whitespace) after trimming.
    Empty,
}

impl std::fmt::Display for ManualModelIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "model id must not be empty"),
        }
    }
}

impl std::error::Error for ManualModelIdError {}

/// Accept a manually-entered model id without contacting the provider.
///
/// Per the model-source design this is a "trust on entry" check: discovery
/// degrading to manual entry means the provider can't (or won't) validate
/// model ids for us, so we only perform a lightweight local check (id is
/// non-empty once trimmed) rather than attempting a live call.
pub fn accept_manual_model_id(id: &str) -> Result<String, ManualModelIdError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(ManualModelIdError::Empty);
    }
    Ok(trimmed.to_string())
}

/// Attempt to discover the models exposed by `provider`.
///
/// This never hard-errors: any failure (unsupported endpoint, network
/// error, empty result) degrades to [`DiscoveryResult::ManualEntryRequired`]
/// so the UI can prompt for a free-text model id instead.
pub async fn discover(provider: &dyn LlmProvider) -> DiscoveryResult {
    match provider.list_models().await {
        Ok(models) if !models.is_empty() => DiscoveryResult::Discovered(models),
        Ok(_) => DiscoveryResult::ManualEntryRequired {
            reason: "provider returned no models".to_string(),
        },
        Err(err) => DiscoveryResult::ManualEntryRequired {
            reason: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{LlmRequest, LlmResponse};
    use anyhow::{bail, Result};
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    /// A fake provider whose `list_models` behavior is fixed at
    /// construction time, so discovery logic can be exercised without any
    /// live network calls.
    struct FakeProvider {
        models: Result<Vec<String>>,
    }

    impl FakeProvider {
        fn listing(models: Vec<String>) -> Self {
            Self { models: Ok(models) }
        }

        fn failing(message: &str) -> Self {
            Self {
                models: Err(anyhow::anyhow!(message.to_string())),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> Result<LlmResponse> {
            bail!("chat not implemented for FakeProvider")
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            match &self.models {
                Ok(models) => Ok(models.clone()),
                Err(err) => Err(anyhow::anyhow!(err.to_string())),
            }
        }

        async fn is_available(&self) -> bool {
            self.models.is_ok()
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }
    }

    #[tokio::test]
    async fn discover_returns_discovered_when_provider_lists_models() {
        let provider = FakeProvider::listing(vec!["a".to_string(), "b".to_string()]);

        let result = discover(&provider).await;

        assert_eq!(
            result,
            DiscoveryResult::Discovered(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[tokio::test]
    async fn discover_falls_back_to_manual_entry_when_list_models_errors() {
        let provider = FakeProvider::failing("listing not supported");

        let result = discover(&provider).await;

        match result {
            DiscoveryResult::ManualEntryRequired { reason } => {
                assert!(
                    reason.contains("listing not supported"),
                    "reason should surface the underlying error, got: {reason}"
                );
            }
            other => panic!("expected ManualEntryRequired, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn discover_falls_back_to_manual_entry_when_list_is_empty() {
        let provider = FakeProvider::listing(vec![]);

        let result = discover(&provider).await;

        assert!(
            result.is_manual(),
            "an empty model list should still require manual entry, got {result:?}"
        );
    }

    #[test]
    fn accept_manual_model_id_trims_and_accepts_nonempty_id() {
        let accepted = accept_manual_model_id("  gpt-4o-mini  ").expect("should accept id");
        assert_eq!(accepted, "gpt-4o-mini");
    }

    #[test]
    fn accept_manual_model_id_rejects_blank_input() {
        let err = accept_manual_model_id("   ").unwrap_err();
        assert_eq!(err, ManualModelIdError::Empty);
    }
}
