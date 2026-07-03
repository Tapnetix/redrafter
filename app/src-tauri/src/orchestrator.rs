// The refine pipeline: capture the current selection, parse any inline
// commands out of it (`/rd`, `/m`, `/q`, `/lang` — B4's `command_parser`,
// falling back to `quote_parser`'s heuristic quote detection when there's
// no explicit `/q`), build a prompt, call the active model — retrying an
// ordered fallback chain if it fails — then either inject the result
// immediately (blind mode) or suspend for the user to accept/edit/discard
// it (review mode).
//
// A5 built the narrow capture -> prompt -> model -> inject pipeline with
// its seams (`TextCapture`/`TextInjector`/`LlmProvider`) kept generic so B5
// could extend it without a rewrite; this file is that extension.
// `refine` (A5's original entry point) is kept as a thin wrapper over
// `refine_with` (no fallback targets, blind mode) so its signature and
// behavior for a plain untagged selection are unchanged.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/orchestrator_test.rs` via `include!` inside an inline `mod` block,
// and Rust doesn't allow an inner doc comment produced by macro expansion
// to sit at the start of that block.)

use std::sync::{Arc, Mutex};

use anyhow::Result;
use llm_provider::LlmProvider;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::command_parser;
use crate::prompt_builder::{self, BuildOptions, QuoteMode};
use crate::quote_parser;

/// Captures the user's current text selection. A seam over
/// `text_inject::capture` so the pipeline can be driven by a fake in tests
/// instead of touching real Accessibility/clipboard APIs.
pub trait TextCapture: Send + Sync {
    fn capture(&self) -> Result<String>;
}

/// Injects text into the focused app. A seam over `text_inject::inject`,
/// mirroring [`TextCapture`].
pub trait TextInjector: Send + Sync {
    fn inject(&self, text: &str) -> Result<()>;
}

/// Production [`TextCapture`]/[`TextInjector`] backed by the real
/// `text-inject` crate (Accessibility API, clipboard fallback).
pub struct SystemTextIo;

impl TextCapture for SystemTextIo {
    fn capture(&self) -> Result<String> {
        Ok(text_inject::capture()?.text)
    }
}

impl TextInjector for SystemTextIo {
    fn inject(&self, text: &str) -> Result<()> {
        text_inject::inject(text)
    }
}

/// The result of a successful [`Orchestrator::refine`] call, consumed by
/// the frontend/tray.
///
/// `Serialize` is derived (rather than left for A14 to wrap) so this type
/// can be returned directly from the `refine` Tauri command A14's
/// composition root (`lib.rs`) registers; field names already match the
/// frontend's `RefineOutcome` (`app/src/lib/ipc.ts`) with no renaming
/// needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefineOutcome {
    pub original: String,
    pub refined: String,
    pub model: String,
}

/// One additional candidate in the ordered model fallback chain: a
/// provider to call and the specific model id to request from it. Tried
/// only if the primary attempt (`Orchestrator`'s own provider, with
/// `opts.model`) fails, in the order given.
///
/// The fallback list itself (which models, in what order) comes from the
/// Behavior screen's `behavior.on_failure`/`behavior.fallback_chain`
/// settings; the command layer (`lib.rs`'s `resolve_fallback_targets`) reads
/// those and constructs this list when on-failure is set to "fallback". Kept
/// as a plain data seam here so the chain-walking logic is trivial to unit
/// test with fakes, independent of how the command layer sources it.
pub struct FallbackTarget {
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
}

impl FallbackTarget {
    pub fn new(provider: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }
}

/// Whether a successfully refined result is injected immediately or held
/// for the user to accept/edit/discard first.
///
/// The active mode comes from settings/behavior config (B23 wires); `refine`
/// (A5's original entry point) always uses `Blind`, matching Phase A's only
/// behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InjectMode {
    /// Inject the model's output immediately — Phase A's only behavior.
    #[default]
    Blind,
    /// Suspend after the model returns: [`Orchestrator::refine_with`]
    /// records the result as pending and returns
    /// [`RefineFlow::PendingReview`] without injecting anything. The
    /// command layer (B23) + Capture review UI (B6/B16) then drive
    /// [`Orchestrator::accept`]/[`Orchestrator::edit`]/
    /// [`Orchestrator::discard`] from the user's choice.
    Review,
}

/// The outcome of [`Orchestrator::refine_with`]: either already injected
/// (`InjectMode::Blind`) or awaiting the user's accept/edit/discard
/// decision (`InjectMode::Review`). Both variants carry the same
/// [`RefineOutcome`] — [`RefineFlow::into_outcome`] unwraps either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RefineFlow {
    Injected(RefineOutcome),
    PendingReview(RefineOutcome),
}

impl RefineFlow {
    /// The wrapped outcome, regardless of variant.
    pub fn into_outcome(self) -> RefineOutcome {
        match self {
            RefineFlow::Injected(outcome) | RefineFlow::PendingReview(outcome) => outcome,
        }
    }
}

/// Parses `original`'s inline commands (B4's `command_parser`) and resolves
/// its quoted context — an explicit `/q` tag if present, otherwise the
/// Behavior screen's `quote_mode` decides whether to run `quote_parser`'s
/// heuristic detection ([`QuoteMode::AnswerOnly`]) or leave the selection
/// whole ([`QuoteMode::IncludeQuote`]/[`QuoteMode::LetDirectionDecide`]) —
/// into the text to actually refine (`draft`) plus a [`BuildOptions`] with
/// `direction`/`quote`/`lang` resolved (a parsed tag overrides the
/// corresponding field already set on `opts`, e.g. a settings-configured
/// default direction; `model`/`temperature`/`max_tokens`/`quote_mode` are
/// untouched, since those aren't tag-driven).
///
/// With no tags at all, `command_parser::parse` returns the trimmed
/// selection as `message` and everything else `None`, so `direction`/`lang`
/// are a no-op over `opts`; only `quote`/`draft` depend on `quote_mode`.
///
/// A plain function of its inputs (no `&self`, no I/O) so it's trivial to
/// unit test on its own, independent of the async model-calling machinery.
fn resolve_prompt(original: &str, opts: &BuildOptions) -> (String, BuildOptions) {
    let parsed = command_parser::parse(original);

    let (quote, draft) = match parsed.quote {
        // An explicit `/q` tag always wins, regardless of `quote_mode`.
        Some(explicit_quote) => (Some(explicit_quote), parsed.message),
        None => match opts.quote_mode {
            // "Answer only": strip the heuristically-detected quote out and
            // refine only the user's own words.
            QuoteMode::AnswerOnly => quote_parser::split(&parsed.message),
            // "Answer + quote" / "Let /rd decide": don't auto-strip — the
            // whole selection is the draft, no heuristic quote separation.
            QuoteMode::IncludeQuote | QuoteMode::LetDirectionDecide => (None, parsed.message),
        },
    };

    let resolved = BuildOptions {
        direction: parsed.direction.or_else(|| opts.direction.clone()),
        model: opts.model.clone(),
        temperature: opts.temperature,
        max_tokens: opts.max_tokens,
        quote: quote.or_else(|| opts.quote.clone()),
        lang: parsed.lang.or_else(|| opts.lang.clone()),
        quote_mode: opts.quote_mode,
    };

    (draft, resolved)
}

/// Orchestrates the refine pipeline: capture -> parse/resolve prompt ->
/// call model (with fallback) -> inject (or suspend for review).
///
/// The most recently captured selection is kept in an in-memory restore
/// buffer (set as soon as it's captured, before the possibly-failing model
/// call) so [`Orchestrator::restore_original`] can re-inject it even if the
/// refine call itself failed or the user simply wants their original text
/// back. The buffer is only ever read by `restore_original`/
/// `captured_original` — the pipeline never injects anything but the
/// model's output, and never on anything but success.
///
/// A separate `pending_review` slot holds the most recent
/// [`InjectMode::Review`] result awaiting the user's accept/edit/discard
/// decision; unrelated to the restore buffer, since the original selection
/// must stay recoverable independent of what happens to the review.
pub struct Orchestrator<C: TextCapture, I: TextInjector> {
    capture: C,
    injector: I,
    provider: Arc<dyn LlmProvider>,
    restore_buffer: Mutex<Option<String>>,
    pending_review: Mutex<Option<RefineOutcome>>,
}

impl<C: TextCapture, I: TextInjector> Orchestrator<C, I> {
    pub fn new(capture: C, injector: I, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            capture,
            injector,
            provider,
            restore_buffer: Mutex::new(None),
            pending_review: Mutex::new(None),
        }
    }

    /// Runs the default refine pipeline once: capture the selection, parse/
    /// build a prompt from `opts`, call the active model (honoring
    /// `cancel`), and inject the model's output. A thin wrapper over
    /// [`Orchestrator::refine_with`] with no fallback targets and
    /// [`InjectMode::Blind`] — A5's original entry point, kept with its
    /// original signature and blind-inject behavior.
    ///
    /// The captured original is never mutated or injected — only the
    /// model's response is injected, and only after the model call
    /// succeeds. On any failure (capture, model call, or inject) nothing
    /// is injected; the caller sees the error and the user's on-screen
    /// text is left untouched.
    pub async fn refine(
        &self,
        opts: &BuildOptions,
        cancel: CancellationToken,
    ) -> Result<RefineOutcome> {
        let flow = self.refine_with(opts, &[], InjectMode::Blind, cancel).await?;
        Ok(flow.into_outcome())
    }

    /// Runs the full refine pipeline: capture the selection, parse its
    /// inline commands/quote (`resolve_prompt`), call the active model
    /// (`opts.model` via the primary provider), retrying `fallbacks` in
    /// order on failure, then either inject the result immediately
    /// (`InjectMode::Blind`) or record it as pending review
    /// (`InjectMode::Review`) without injecting anything.
    ///
    /// The captured original is stashed in the restore buffer before the
    /// model call (the step most likely to fail) so it survives every
    /// attempt in the fallback chain, and is never itself injected or
    /// mutated. Cancellation is honored both within each provider call and
    /// between fallback attempts — a cancelled token stops the chain
    /// rather than working through the remaining models regardless.
    pub async fn refine_with(
        &self,
        opts: &BuildOptions,
        fallbacks: &[FallbackTarget],
        mode: InjectMode,
        cancel: CancellationToken,
    ) -> Result<RefineFlow> {
        let original = self.capture.capture()?;

        // Stash the original before the model call, which is the step most
        // likely to fail (network, auth, cancellation) — the restore
        // buffer must hold it regardless of whether the rest of the
        // pipeline (including every fallback attempt) succeeds.
        *self.restore_buffer.lock().unwrap() = Some(original.clone());

        let (draft, resolved_opts) = resolve_prompt(&original, opts);
        let response = self
            .call_with_fallback(&draft, &resolved_opts, fallbacks, cancel)
            .await?;

        let outcome = RefineOutcome {
            original,
            refined: response.text,
            model: response.model,
        };

        match mode {
            InjectMode::Blind => {
                self.injector.inject(&outcome.refined)?;
                Ok(RefineFlow::Injected(outcome))
            }
            InjectMode::Review => {
                *self.pending_review.lock().unwrap() = Some(outcome.clone());
                Ok(RefineFlow::PendingReview(outcome))
            }
        }
    }

    /// Calls the primary provider (`resolved_opts.model`), falling back
    /// through `fallbacks` in order on failure until one succeeds or the
    /// list is exhausted. Checks `cancel` before each fallback attempt (each
    /// provider's own `chat` already honors `cancel` mid-flight) so a
    /// cancellation doesn't burn through the rest of the chain regardless.
    ///
    /// On total exhaustion, returns the last error encountered (the
    /// caller's generic-failure branch — nothing is injected and the
    /// restore buffer is untouched by this method).
    async fn call_with_fallback(
        &self,
        draft: &str,
        resolved_opts: &BuildOptions,
        fallbacks: &[FallbackTarget],
        cancel: CancellationToken,
    ) -> Result<llm_provider::LlmResponse> {
        let primary_request = prompt_builder::build(draft, resolved_opts);
        let mut last_err = match self.provider.chat(&primary_request, cancel.clone()).await {
            Ok(response) => return Ok(response),
            Err(err) => err,
        };

        for target in fallbacks {
            if cancel.is_cancelled() {
                return Err(last_err.context("refine was cancelled during the fallback chain"));
            }

            let mut target_opts = resolved_opts.clone();
            target_opts.model = target.model.clone();
            let request = prompt_builder::build(draft, &target_opts);

            match target.provider.chat(&request, cancel.clone()).await {
                Ok(response) => return Ok(response),
                Err(err) => last_err = err,
            }
        }

        Err(last_err)
    }

    /// Returns the most recently captured original selection, if any.
    pub fn captured_original(&self) -> Option<String> {
        self.restore_buffer.lock().unwrap().clone()
    }

    /// Re-injects the original text captured by the most recent
    /// [`Orchestrator::refine`]/[`Orchestrator::refine_with`] call. Errors
    /// if nothing has been captured yet.
    pub fn restore_original(&self) -> Result<()> {
        let original = self
            .captured_original()
            .ok_or_else(|| anyhow::anyhow!("no captured original to restore"))?;
        self.injector.inject(&original)
    }

    /// Returns the pending review result, if the most recent
    /// [`Orchestrator::refine_with`] call ran in [`InjectMode::Review`] and
    /// hasn't been accepted/edited/discarded yet.
    pub fn pending_review(&self) -> Option<RefineOutcome> {
        self.pending_review.lock().unwrap().clone()
    }

    /// Accepts the pending review result: injects `text` (the model's
    /// unedited output, or the user's edited version of it — the caller
    /// decides which) and clears the pending state. Errors if nothing is
    /// pending review; if the injector itself fails, the pending state is
    /// left in place (rather than silently discarded) so the caller can
    /// retry `accept`/`edit`/`discard` instead of losing the result.
    pub fn accept(&self, text: &str) -> Result<()> {
        self.require_pending_review()?;
        self.injector.inject(text)?;
        *self.pending_review.lock().unwrap() = None;
        Ok(())
    }

    /// Injects `new_text` in place of the pending review result. An alias
    /// for [`Orchestrator::accept`] with a name that matches what the user
    /// actually did (edited the draft before committing it) — the command
    /// layer (B23)/Capture review UI (B6/B16) call whichever name matches
    /// the user's action; behavior is identical either way.
    pub fn edit(&self, new_text: &str) -> Result<()> {
        self.accept(new_text)
    }

    /// Discards the pending review result: clears it without ever calling
    /// the injector, leaving whatever is on-screen (the user's original
    /// selection) untouched. Errors if nothing is pending review.
    pub fn discard(&self) -> Result<()> {
        self.require_pending_review()?;
        *self.pending_review.lock().unwrap() = None;
        Ok(())
    }

    /// Errors unless a refine result is currently pending review — shared
    /// by `accept`/`discard`, which only clear the pending state (see each
    /// method) once their own action (inject, or nothing) has succeeded.
    fn require_pending_review(&self) -> Result<()> {
        if self.pending_review.lock().unwrap().is_none() {
            anyhow::bail!("no refine result pending review");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A selection with an unambiguous quoted reply block plus the user's
    /// own draft line -- the heuristic splitter (`quote_parser::split`)
    /// detects the quote here.
    const QUOTED_SELECTION: &str =
        "> On Mon, Alex wrote: any risk of slipping?\n\nWe're on track, shipping Monday.";

    fn opts_with_quote_mode(quote_mode: QuoteMode) -> BuildOptions {
        BuildOptions {
            model: "fake-model".to_string(),
            quote_mode,
            ..BuildOptions::default()
        }
    }

    #[test]
    fn answer_only_quote_mode_splits_the_detected_quote_out_of_the_draft() {
        let (draft, resolved) =
            resolve_prompt(QUOTED_SELECTION, &opts_with_quote_mode(QuoteMode::AnswerOnly));

        // Only the user's own words remain in the draft; the quoted reply is
        // pulled out as reference-only context.
        assert_eq!(draft, "We're on track, shipping Monday.");
        assert_eq!(
            resolved.quote.as_deref(),
            Some("> On Mon, Alex wrote: any risk of slipping?")
        );
    }

    #[test]
    fn include_quote_mode_leaves_the_whole_selection_as_the_draft() {
        let (draft, resolved) =
            resolve_prompt(QUOTED_SELECTION, &opts_with_quote_mode(QuoteMode::IncludeQuote));

        // No heuristic split: the whole selection is refined, no quote pulled
        // out.
        assert_eq!(draft, QUOTED_SELECTION);
        assert_eq!(resolved.quote, None);
    }

    #[test]
    fn let_direction_decide_quote_mode_also_skips_the_heuristic_split() {
        let (draft, resolved) = resolve_prompt(
            QUOTED_SELECTION,
            &opts_with_quote_mode(QuoteMode::LetDirectionDecide),
        );

        assert_eq!(draft, QUOTED_SELECTION);
        assert_eq!(resolved.quote, None);
    }

    #[test]
    fn an_explicit_q_tag_wins_over_the_quote_mode() {
        // Even in a mode that would otherwise skip the heuristic split, an
        // explicit `/q` tag is always honored.
        let selection = "/q previous thread context /m my actual draft";
        let (draft, resolved) =
            resolve_prompt(selection, &opts_with_quote_mode(QuoteMode::IncludeQuote));

        assert_eq!(resolved.quote.as_deref(), Some("previous thread context"));
        assert_eq!(draft, "my actual draft");
    }
}
