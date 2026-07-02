// Pure assembly of the refine `LlmRequest` from a direction (system prompt)
// and the user's captured selection (user prompt). No I/O: `build` is a
// plain function of its inputs so it's trivial to unit test, and so the
// orchestrator can call it without needing a fake for this seam.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/orchestrator_test.rs` via `include!` inside an inline `mod` block,
// and Rust doesn't allow an inner doc comment produced by macro expansion
// to sit at the start of that block.)

use llm_provider::{ChatMessage, LlmRequest};

/// The editable default refine direction: a light grammar/clarity pass
/// that preserves the author's voice and length. Command parsing that lets
/// the user override this per-invocation (`/rd`, quoted directions, etc.)
/// is a Phase B concern (B4/B5); this is just the default the settings UI
/// lets the user edit.
pub const DEFAULT_DIRECTION: &str = "Lightly edit the following text for grammar, spelling, \
and clarity. Preserve the author's voice, tone, and overall length — this is a polish, not a \
rewrite. Do not summarize or change the meaning. Reply with only the corrected text: no \
commentary, preamble, or surrounding quotation marks.";

/// Options controlling how a refine prompt is assembled.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// The refine direction/system instructions. `None` falls back to
    /// [`DEFAULT_DIRECTION`].
    pub direction: Option<String>,
    /// The model identifier to request (the active model from
    /// `llm-provider`/settings).
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            direction: None,
            model: String::new(),
            temperature: 0.3,
            max_tokens: None,
        }
    }
}

/// Assembles a system+user [`LlmRequest`] from `opts` and the user's
/// captured selection (`input`). Pure function of its inputs: no command
/// parsing, no I/O, so it's trivial to unit test and safe for the
/// orchestrator to call synchronously.
pub fn build(input: &str, opts: &BuildOptions) -> LlmRequest {
    let direction = opts.direction.as_deref().unwrap_or(DEFAULT_DIRECTION);
    LlmRequest {
        messages: vec![ChatMessage::system(direction), ChatMessage::user(input)],
        model: opts.model.clone(),
        temperature: opts.temperature,
        max_tokens: opts.max_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_direction_appears_in_the_system_message() {
        let request = build("some selected text", &BuildOptions::default());

        let system = &request.messages[0];
        assert_eq!(system.role, "system");
        assert_eq!(system.content, DEFAULT_DIRECTION);
    }
}
