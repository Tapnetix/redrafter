//! Compiles the Phase A refine-pipeline modules (`prompt_builder`,
//! `orchestrator`) ahead of A14 wiring them into `lib.rs`'s module tree
//! (A14 owns the composition root, which this task does not touch), and
//! exercises the orchestrator's `refine`/`restore_original` pipeline with
//! fakes for the LLM provider and the text-inject capture/inject seam.
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/orchestrator.rs"]` — see
//! `tests/core_test.rs` for why: a relative `#[path]` from a file under
//! `tests/` embeds a literal, unnormalized `tests/../src/...` path in debug
//! info, which `cargo llvm-cov`'s default ignore rule for `tests/` silently
//! filters out — zeroing coverage for this task's real production code. The
//! absolute path here resolves to plain `.../src/*.rs`, so coverage
//! attributes correctly.

mod command_parser {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/command_parser.rs"
    ));
}
mod quote_parser {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/quote_parser.rs"));
}
mod prompt_builder {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/prompt_builder.rs"
    ));
}
mod orchestrator {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/orchestrator.rs"));
}

// Wrapped in a module whose name contains "orchestrator" so the plan's
// verification filter (`cargo nextest run -p redrafter orchestrator
// prompt_builder`) discovers these tests by substring match on the
// qualified test name — mirroring how `prompt_builder`'s own `#[cfg(test)]
// mod tests` (embedded above via `include!`) is discoverable through its
// module path already containing "prompt_builder".
mod orchestrator_pipeline_tests {
    use super::orchestrator::{Orchestrator, TextCapture, TextInjector};
    use super::prompt_builder::BuildOptions;
    use anyhow::Result;
    use async_trait::async_trait;
    use llm_provider::{LlmProvider, LlmRequest, LlmResponse};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    /// Fake capture that always returns a fixed selection.
    struct FakeCapture(String);

    impl TextCapture for FakeCapture {
        fn capture(&self) -> Result<String> {
            Ok(self.0.clone())
        }
    }

    /// Fake capture that always fails, to exercise the "no selection" path.
    struct FailingCapture;

    impl TextCapture for FailingCapture {
        fn capture(&self) -> Result<String> {
            anyhow::bail!("no selection available")
        }
    }

    /// Fake injector that records every string it's asked to inject, so
    /// tests can assert on it. Cloning shares the same underlying log
    /// (`Arc`), so a clone can be handed to the `Orchestrator` while the
    /// original stays with the test for assertions.
    #[derive(Clone, Default)]
    struct FakeInjector {
        injected: Arc<Mutex<Vec<String>>>,
    }

    impl FakeInjector {
        fn injected(&self) -> Vec<String> {
            self.injected.lock().unwrap().clone()
        }
    }

    impl TextInjector for FakeInjector {
        fn inject(&self, text: &str) -> Result<()> {
            self.injected.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    /// Fake provider that echoes back a canned response.
    struct FakeProvider(String);

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> Result<LlmResponse> {
            Ok(LlmResponse {
                text: self.0.clone(),
                model: "fake-model".to_string(),
                usage_tokens: None,
            })
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec!["fake-model".to_string()])
        }

        async fn is_available(&self) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }
    }

    /// Fake provider that always fails, to exercise the "model call
    /// failed" path.
    struct FailingProvider;

    #[async_trait]
    impl LlmProvider for FailingProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> Result<LlmResponse> {
            anyhow::bail!("model call failed")
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec![])
        }

        async fn is_available(&self) -> bool {
            false
        }

        fn provider_name(&self) -> &'static str {
            "failing"
        }
    }

    fn opts() -> BuildOptions {
        BuildOptions {
            model: "fake-model".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn refine_injects_the_model_output() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(FakeProvider("refined text".to_string())),
        );

        let outcome = orch
            .refine(&opts(), CancellationToken::new())
            .await
            .expect("refine should succeed");

        assert_eq!(outcome.original, "original text");
        assert_eq!(outcome.refined, "refined text");
        assert_eq!(injector.injected(), vec!["refined text".to_string()]);
    }

    #[tokio::test]
    async fn refine_stores_the_original_in_the_restore_buffer() {
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            FakeInjector::default(),
            Arc::new(FakeProvider("refined text".to_string())),
        );

        orch.refine(&opts(), CancellationToken::new())
            .await
            .expect("refine should succeed");

        assert_eq!(orch.captured_original(), Some("original text".to_string()));
    }

    #[tokio::test]
    async fn restore_original_reinjects_the_captured_selection() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(FakeProvider("refined text".to_string())),
        );

        orch.refine(&opts(), CancellationToken::new())
            .await
            .expect("refine should succeed");
        orch.restore_original().expect("restore should succeed");

        assert_eq!(
            injector.injected(),
            vec!["refined text".to_string(), "original text".to_string()]
        );
    }

    #[tokio::test]
    async fn restore_original_errors_when_nothing_was_ever_captured() {
        let orch = Orchestrator::new(
            FakeCapture("unused".to_string()),
            FakeInjector::default(),
            Arc::new(FakeProvider("unused".to_string())),
        );

        assert!(orch.restore_original().is_err());
    }

    #[tokio::test]
    async fn refine_does_not_inject_when_the_model_call_fails() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(FailingProvider),
        );

        let result = orch.refine(&opts(), CancellationToken::new()).await;

        assert!(result.is_err());
        assert!(
            injector.injected().is_empty(),
            "must never inject when the model call failed"
        );
        // The original must still be recoverable even though refine failed.
        assert_eq!(orch.captured_original(), Some("original text".to_string()));
    }

    #[tokio::test]
    async fn refine_propagates_a_capture_failure_without_calling_the_model() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FailingCapture,
            injector.clone(),
            Arc::new(FakeProvider("should not be used".to_string())),
        );

        let result = orch.refine(&opts(), CancellationToken::new()).await;

        assert!(result.is_err());
        assert!(injector.injected().is_empty());
        assert_eq!(orch.captured_original(), None);
    }
}
