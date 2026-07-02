// The default refine pipeline: capture the current selection, build a
// prompt from the (configurable) default direction, call the active
// model, then inject the result back where the selection came from.
//
// This module covers Phase A's default-refine path only. Command parsing
// (`/rd`, `//m`, `/q`, `/lang`), quote handling, review mode, and provider
// fallback chains are Phase B (B4/B5) concerns; `refine` is kept narrow and
// its seams (`TextCapture`/`TextInjector`/`LlmProvider`) generic so B5 can
// wrap or extend this pipeline (e.g. a review-mode branch that calls
// `prompt_builder::build` with a different `BuildOptions` before deciding
// whether to inject) without having to rewrite it.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/orchestrator_test.rs` via `include!` inside an inline `mod` block,
// and Rust doesn't allow an inner doc comment produced by macro expansion
// to sit at the start of that block.)

use std::sync::{Arc, Mutex};

use anyhow::Result;
use llm_provider::LlmProvider;
use tokio_util::sync::CancellationToken;

use crate::prompt_builder::{self, BuildOptions};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineOutcome {
    pub original: String,
    pub refined: String,
    pub model: String,
}

/// Orchestrates the default refine pipeline: capture -> build prompt ->
/// call model -> inject.
///
/// The most recently captured selection is kept in an in-memory restore
/// buffer (set as soon as it's captured, before the possibly-failing model
/// call) so [`Orchestrator::restore_original`] can re-inject it even if the
/// refine call itself failed or the user simply wants their original text
/// back. The buffer is only ever read by `restore_original`/
/// `captured_original` — `refine` never injects anything but the model's
/// output, and never on anything but success.
pub struct Orchestrator<C: TextCapture, I: TextInjector> {
    capture: C,
    injector: I,
    provider: Arc<dyn LlmProvider>,
    restore_buffer: Mutex<Option<String>>,
}

impl<C: TextCapture, I: TextInjector> Orchestrator<C, I> {
    pub fn new(capture: C, injector: I, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            capture,
            injector,
            provider,
            restore_buffer: Mutex::new(None),
        }
    }

    /// Runs the default refine pipeline once: capture the selection, build
    /// a prompt from `opts`, call the active model (honoring `cancel`), and
    /// inject the model's output.
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
        let original = self.capture.capture()?;

        // Stash the original before the model call, which is the step most
        // likely to fail (network, auth, cancellation) — the restore
        // buffer must hold it regardless of whether the rest of the
        // pipeline succeeds.
        *self.restore_buffer.lock().unwrap() = Some(original.clone());

        let request = prompt_builder::build(&original, opts);
        let response = self.provider.chat(&request, cancel).await?;

        self.injector.inject(&response.text)?;

        Ok(RefineOutcome {
            original,
            refined: response.text,
            model: response.model,
        })
    }

    /// Returns the most recently captured original selection, if any.
    pub fn captured_original(&self) -> Option<String> {
        self.restore_buffer.lock().unwrap().clone()
    }

    /// Re-injects the original text captured by the most recent
    /// [`Orchestrator::refine`] call. Errors if nothing has been captured
    /// yet.
    pub fn restore_original(&self) -> Result<()> {
        let original = self
            .captured_original()
            .ok_or_else(|| anyhow::anyhow!("no captured original to restore"))?;
        self.injector.inject(&original)
    }
}
