//! Exercises B5's extensions to `orchestrator.rs`: wiring B4's
//! `command_parser`/`quote_parser` into the prompt built for the model,
//! the ordered model fallback chain, and the review-and-confirm branch
//! (`InjectMode::Review` + `accept`/`edit`/`discard`).
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
mod presets {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/presets.rs"));
}
mod orchestrator {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/orchestrator.rs"));
}

// Wrapped in a module whose name contains "fallback" so the plan's
// verification filter (`cargo nextest run -p redrafter orchestrator
// fallback`) discovers these tests by substring match on the qualified test
// name — mirroring `orchestrator_test.rs`/`parser_test.rs`'s own naming
// convention.
mod fallback_and_review_tests {
    use super::orchestrator::{
        FallbackTarget, InjectMode, Orchestrator, RefineFlow, TextCapture, TextInjector,
    };
    use super::prompt_builder::{BuildOptions, QuoteMode};
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

    /// Fake injector that records every string it's asked to inject.
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

    /// Fake provider that always fails with a fixed message, to exercise
    /// fallback and total-exhaustion paths.
    struct FailingProvider(&'static str);

    #[async_trait]
    impl LlmProvider for FailingProvider {
        async fn chat(&self, _request: &LlmRequest, _cancel: CancellationToken) -> Result<LlmResponse> {
            anyhow::bail!(self.0)
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

    /// Fake provider that succeeds, echoing back the model name it was
    /// constructed with (so a test can tell which chain link answered) and
    /// recording the last request it received (so a test can assert on how
    /// the prompt was built).
    #[derive(Clone, Default)]
    struct RecordingProvider {
        model: &'static str,
        last_request: Arc<Mutex<Option<LlmRequest>>>,
    }

    impl RecordingProvider {
        fn new(model: &'static str) -> Self {
            Self {
                model,
                last_request: Arc::new(Mutex::new(None)),
            }
        }

        fn last_request(&self) -> LlmRequest {
            self.last_request
                .lock()
                .unwrap()
                .clone()
                .expect("provider should have been called")
        }
    }

    #[async_trait]
    impl LlmProvider for RecordingProvider {
        async fn chat(&self, request: &LlmRequest, _cancel: CancellationToken) -> Result<LlmResponse> {
            *self.last_request.lock().unwrap() = Some(request.clone());
            Ok(LlmResponse {
                text: format!("refined by {}", self.model),
                model: self.model.to_string(),
                usage_tokens: None,
            })
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec![self.model.to_string()])
        }

        async fn is_available(&self) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "recording"
        }
    }

    /// Fake provider that fails its first `fail_first` calls and then
    /// succeeds on every subsequent call, counting total calls — to exercise
    /// the per-model retry loop (a transient failure that a retry recovers
    /// from vs. one that exhausts the retries and falls through).
    struct FlakyProvider {
        model: &'static str,
        fail_first: u32,
        calls: Arc<Mutex<u32>>,
    }

    impl FlakyProvider {
        fn new(model: &'static str, fail_first: u32) -> Self {
            Self {
                model,
                fail_first,
                calls: Arc::new(Mutex::new(0)),
            }
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl LlmProvider for FlakyProvider {
        async fn chat(&self, _request: &LlmRequest, _cancel: CancellationToken) -> Result<LlmResponse> {
            let call = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            if call <= self.fail_first {
                anyhow::bail!("{} transiently failed on attempt {call}", self.model);
            }
            Ok(LlmResponse {
                text: format!("refined by {}", self.model),
                model: self.model.to_string(),
                usage_tokens: None,
            })
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec![self.model.to_string()])
        }

        async fn is_available(&self) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "flaky"
        }
    }

    fn opts(model: &str) -> BuildOptions {
        BuildOptions {
            model: model.to_string(),
            ..Default::default()
        }
    }

    // ---- Fallback chain ----

    #[tokio::test]
    async fn falls_back_to_the_next_model_when_the_primary_fails() {
        let injector = FakeInjector::default();
        let fallback = RecordingProvider::new("fallback-model");
        let orch = Orchestrator::new(
            FakeCapture("please fix this up".to_string()),
            injector.clone(),
            Arc::new(FailingProvider("primary is down")),
        );

        let flow = orch
            .refine_with(
                &opts("primary-model"),
                &[FallbackTarget::new(Arc::new(fallback.clone()), "fallback-model")],
                InjectMode::Blind,
                None,
                1,
                CancellationToken::new(),
            )
            .await
            .expect("should succeed via the fallback model");

        let outcome = flow.into_outcome();
        assert_eq!(outcome.model, "fallback-model");
        assert_eq!(outcome.refined, "refined by fallback-model");
        assert_eq!(injector.injected(), vec!["refined by fallback-model".to_string()]);
        // The original must still be untouched/recoverable.
        assert_eq!(outcome.original, "please fix this up");
    }

    #[tokio::test]
    async fn tries_fallbacks_in_order_and_stops_at_the_first_success() {
        let second = RecordingProvider::new("second-model");
        let orch = Orchestrator::new(
            FakeCapture("draft text".to_string()),
            FakeInjector::default(),
            Arc::new(FailingProvider("primary down")),
        );

        let flow = orch
            .refine_with(
                &opts("primary-model"),
                &[
                    FallbackTarget::new(Arc::new(FailingProvider("first fallback down")), "first-model"),
                    FallbackTarget::new(Arc::new(second.clone()), "second-model"),
                ],
                InjectMode::Blind,
                None,
                1,
                CancellationToken::new(),
            )
            .await
            .expect("should succeed at the second fallback");

        assert_eq!(flow.into_outcome().model, "second-model");
    }

    #[tokio::test]
    async fn original_is_untouched_when_every_model_in_the_chain_fails() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(FailingProvider("primary down")),
        );

        let result = orch
            .refine_with(
                &opts("primary-model"),
                &[FallbackTarget::new(
                    Arc::new(FailingProvider("fallback down too")),
                    "fallback-model",
                )],
                InjectMode::Blind,
                None,
                1,
                CancellationToken::new(),
            )
            .await;

        assert!(result.is_err());
        assert!(
            injector.injected().is_empty(),
            "must never inject when the whole fallback chain is exhausted"
        );
        assert_eq!(orch.captured_original(), Some("original text".to_string()));
    }

    #[tokio::test]
    async fn cancellation_stops_the_chain_before_trying_the_next_fallback() {
        let cancel = CancellationToken::new();
        let never_called = RecordingProvider::new("never-called-model");
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            FakeInjector::default(),
            Arc::new(FailingProvider("primary down")),
        );

        // Cancel before the primary attempt even starts: the fallback loop
        // must observe this and refuse to try any further model, rather than
        // burning through the whole chain regardless.
        cancel.cancel();

        let result = orch
            .refine_with(
                &opts("primary-model"),
                &[FallbackTarget::new(Arc::new(never_called.clone()), "never-called-model")],
                InjectMode::Blind,
                None,
                1,
                cancel,
            )
            .await;

        assert!(result.is_err());
        assert!(
            never_called.last_request.lock().unwrap().is_none(),
            "cancellation must stop the chain before a later fallback is tried"
        );
    }

    // ---- Per-model retry (behavior.retry_count) ----

    #[tokio::test]
    async fn retry_count_2_retries_the_same_model_before_advancing_to_the_fallback() {
        // The primary fails once then succeeds. With `attempts = 2` it gets a
        // second shot at the *same* model and answers — the fallback is never
        // reached.
        let primary = Arc::new(FlakyProvider::new("primary-model", 1));
        let fallback = RecordingProvider::new("fallback-model");
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("please fix this up".to_string()),
            injector.clone(),
            primary.clone(),
        );

        let flow = orch
            .refine_with(
                &opts("primary-model"),
                &[FallbackTarget::new(Arc::new(fallback.clone()), "fallback-model")],
                InjectMode::Blind,
                None,
                2,
                CancellationToken::new(),
            )
            .await
            .expect("the primary should succeed on its retry");

        let outcome = flow.into_outcome();
        // The primary model answered (via its retry), not the fallback.
        assert_eq!(outcome.model, "primary-model");
        assert_eq!(injector.injected(), vec!["refined by primary-model".to_string()]);
        // Two attempts against the primary (fail, then success)...
        assert_eq!(primary.call_count(), 2);
        // ...and the fallback was never tried.
        assert!(
            fallback.last_request.lock().unwrap().is_none(),
            "the fallback must not be reached once a retry recovers the primary"
        );
    }

    #[tokio::test]
    async fn retry_count_exhausted_advances_to_the_fallback() {
        // The primary fails on every attempt. With `attempts = 2` it's tried
        // twice, both fail, and the chain advances to the fallback.
        let primary = Arc::new(FlakyProvider::new("primary-model", u32::MAX));
        let fallback = RecordingProvider::new("fallback-model");
        let orch = Orchestrator::new(
            FakeCapture("please fix this up".to_string()),
            FakeInjector::default(),
            primary.clone(),
        );

        let flow = orch
            .refine_with(
                &opts("primary-model"),
                &[FallbackTarget::new(Arc::new(fallback.clone()), "fallback-model")],
                InjectMode::Blind,
                None,
                2,
                CancellationToken::new(),
            )
            .await
            .expect("the fallback should answer once the primary's retries are exhausted");

        assert_eq!(flow.into_outcome().model, "fallback-model");
        // The primary was tried exactly `attempts` times before advancing.
        assert_eq!(primary.call_count(), 2);
    }

    /// Fake provider that cancels a shared token on its first call and then
    /// fails — to exercise the "cancelled between retries" path: a retry loop
    /// must stop rather than burn through the remaining attempts.
    struct CancellingProvider {
        token: CancellationToken,
        calls: Arc<Mutex<u32>>,
    }

    #[async_trait]
    impl LlmProvider for CancellingProvider {
        async fn chat(&self, _request: &LlmRequest, _cancel: CancellationToken) -> Result<LlmResponse> {
            *self.calls.lock().unwrap() += 1;
            self.token.cancel();
            anyhow::bail!("failed, and the token is now cancelled")
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec![])
        }

        async fn is_available(&self) -> bool {
            false
        }

        fn provider_name(&self) -> &'static str {
            "cancelling"
        }
    }

    #[tokio::test]
    async fn cancellation_between_retries_stops_further_attempts_on_the_same_model() {
        // With `attempts = 3` the primary would normally be tried three times,
        // but the token is cancelled during the first attempt — so the retry
        // loop must stop after that one call rather than retrying.
        let cancel = CancellationToken::new();
        let calls = Arc::new(Mutex::new(0));
        let provider = Arc::new(CancellingProvider {
            token: cancel.clone(),
            calls: calls.clone(),
        });
        let orch = Orchestrator::new(
            FakeCapture("please fix this up".to_string()),
            FakeInjector::default(),
            provider,
        );

        let result = orch
            .refine_with(
                &opts("primary-model"),
                &[],
                InjectMode::Blind,
                None,
                3,
                cancel,
            )
            .await;

        assert!(result.is_err());
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "a token cancelled mid-attempt must stop the retry loop, not burn through every attempt"
        );
    }

    // ---- Review-and-confirm branch ----

    #[tokio::test]
    async fn blind_mode_injects_immediately() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        let flow = orch
            .refine_with(&opts("fake-model"), &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("blind refine should succeed");

        assert!(matches!(flow, RefineFlow::Injected(_)));
        assert_eq!(injector.injected(), vec!["refined by fake-model".to_string()]);
    }

    #[tokio::test]
    async fn review_mode_suspends_without_injecting() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        let flow = orch
            .refine_with(&opts("fake-model"), &[], InjectMode::Review, None, 1, CancellationToken::new())
            .await
            .expect("review refine should succeed");

        assert!(matches!(flow, RefineFlow::PendingReview(_)));
        assert!(
            injector.injected().is_empty(),
            "review mode must not inject before the user decides"
        );
        assert_eq!(
            orch.pending_review().map(|o| o.refined),
            Some("refined by fake-model".to_string())
        );
    }

    #[tokio::test]
    async fn accept_injects_the_possibly_edited_text_and_clears_pending() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Review, None, 1, CancellationToken::new())
            .await
            .expect("review refine should succeed");

        orch.accept("edited by the user").expect("accept should succeed");

        assert_eq!(injector.injected(), vec!["edited by the user".to_string()]);
        assert_eq!(orch.pending_review(), None);
    }

    #[tokio::test]
    async fn edit_injects_new_text_and_clears_pending() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Review, None, 1, CancellationToken::new())
            .await
            .expect("review refine should succeed");

        orch.edit("a further-edited version").expect("edit should succeed");

        assert_eq!(injector.injected(), vec!["a further-edited version".to_string()]);
        assert_eq!(orch.pending_review(), None);
    }

    #[tokio::test]
    async fn discard_injects_nothing_and_leaves_the_original_untouched() {
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Review, None, 1, CancellationToken::new())
            .await
            .expect("review refine should succeed");

        orch.discard().expect("discard should succeed");

        assert!(
            injector.injected().is_empty(),
            "discard must never inject"
        );
        assert_eq!(orch.pending_review(), None);
        // The user's original selection is still recoverable.
        assert_eq!(orch.captured_original(), Some("original text".to_string()));
    }

    /// Fake injector that always fails, to exercise `accept`'s
    /// "don't lose the pending result on injection failure" path.
    struct FailingInjector;

    impl TextInjector for FailingInjector {
        fn inject(&self, _text: &str) -> Result<()> {
            anyhow::bail!("injection failed")
        }
    }

    #[tokio::test]
    async fn accept_leaves_the_pending_result_in_place_when_injection_fails() {
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            FailingInjector,
            Arc::new(RecordingProvider::new("fake-model")),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Review, None, 1, CancellationToken::new())
            .await
            .expect("review refine should succeed");

        assert!(orch.accept("edited text").is_err());
        assert_eq!(
            orch.pending_review().map(|o| o.refined),
            Some("refined by fake-model".to_string()),
            "a failed injection must not silently drop the pending review result"
        );
    }

    #[tokio::test]
    async fn accept_errs_when_nothing_is_pending_review() {
        let orch = Orchestrator::new(
            FakeCapture("unused".to_string()),
            FakeInjector::default(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        assert!(orch.accept("too late").is_err());
    }

    #[tokio::test]
    async fn discard_errs_when_nothing_is_pending_review() {
        let orch = Orchestrator::new(
            FakeCapture("unused".to_string()),
            FakeInjector::default(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        assert!(orch.discard().is_err());
    }

    // ---- Command/quote/language parsing wired into the built prompt ----

    #[tokio::test]
    async fn rd_and_m_tags_in_the_selection_shape_the_request_sent_to_the_model() {
        let provider = RecordingProvider::new("fake-model");
        let orch = Orchestrator::new(
            FakeCapture("/rd make it concise /m we was gonna ship fri".to_string()),
            FakeInjector::default(),
            Arc::new(provider.clone()),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("refine should succeed");

        let request = provider.last_request();
        assert_eq!(request.messages[0].content, "make it concise");
        assert_eq!(
            request.messages.last().unwrap().content,
            "we was gonna ship fri"
        );
    }

    #[tokio::test]
    async fn explicit_q_tag_folds_in_as_reference_only_context() {
        let provider = RecordingProvider::new("fake-model");
        let orch = Orchestrator::new(
            FakeCapture("/q Alex wrote: any risk of slipping? /m we're on track".to_string()),
            FakeInjector::default(),
            Arc::new(provider.clone()),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("refine should succeed");

        let request = provider.last_request();
        let user = request.messages.last().unwrap();
        assert_eq!(user.content, "we're on track");
        assert!(request
            .messages
            .iter()
            .any(|m| m.content.contains("Alex wrote")));
    }

    #[tokio::test]
    async fn heuristic_quote_detection_applies_in_answer_only_mode_when_there_is_no_explicit_q_tag() {
        let provider = RecordingProvider::new("fake-model");
        let selection = "> On Mon, Alex wrote: any risk of slipping?\n\nwe're on track";
        let orch = Orchestrator::new(
            FakeCapture(selection.to_string()),
            FakeInjector::default(),
            Arc::new(provider.clone()),
        );

        // "Answer only" (quote_mode = AnswerOnly) opts into the heuristic
        // split; the default "Answer + quote" refines the whole selection
        // (covered by `include_quote_mode_leaves_the_whole_selection_as_the_draft`).
        let opts = BuildOptions {
            quote_mode: QuoteMode::AnswerOnly,
            ..opts("fake-model")
        };
        orch.refine_with(&opts, &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("refine should succeed");

        let request = provider.last_request();
        let user = request.messages.last().unwrap();
        assert_eq!(user.content, "we're on track");
        assert!(!user.content.contains("Alex wrote"));
        assert!(request
            .messages
            .iter()
            .any(|m| m.content.contains("Alex wrote")));
    }

    #[tokio::test]
    async fn lang_tag_appends_a_target_language_instruction() {
        let provider = RecordingProvider::new("fake-model");
        let orch = Orchestrator::new(
            FakeCapture("/lang de /m Wir sind auf Kurs".to_string()),
            FakeInjector::default(),
            Arc::new(provider.clone()),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("refine should succeed");

        let request = provider.last_request();
        assert!(request.messages[0].content.contains("de"));
        assert_eq!(request.messages.last().unwrap().content, "Wir sind auf Kurs");
    }

    #[tokio::test]
    async fn no_tags_default_direction_behavior_is_unchanged() {
        let provider = RecordingProvider::new("fake-model");
        let orch = Orchestrator::new(
            FakeCapture("just fix this up please".to_string()),
            FakeInjector::default(),
            Arc::new(provider.clone()),
        );

        orch.refine_with(&opts("fake-model"), &[], InjectMode::Blind, None, 1, CancellationToken::new())
            .await
            .expect("refine should succeed");

        let request = provider.last_request();
        assert_eq!(
            request.messages[0].content,
            super::prompt_builder::DEFAULT_DIRECTION
        );
        assert_eq!(
            request.messages.last().unwrap().content,
            "just fix this up please"
        );
    }

    #[tokio::test]
    async fn the_base_refine_method_still_wires_parsing_and_still_blind_injects() {
        // `refine` (A5's original entry point) must keep working exactly as
        // before for plain, untagged selections -- no fallback list, no
        // review suspension.
        let injector = FakeInjector::default();
        let orch = Orchestrator::new(
            FakeCapture("original text".to_string()),
            injector.clone(),
            Arc::new(RecordingProvider::new("fake-model")),
        );

        let outcome = orch
            .refine(&opts("fake-model"), CancellationToken::new())
            .await
            .expect("refine should succeed");

        assert_eq!(outcome.original, "original text");
        assert_eq!(outcome.refined, "refined by fake-model");
        assert_eq!(injector.injected(), vec!["refined by fake-model".to_string()]);
    }
}
