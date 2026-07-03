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
    /// [`DEFAULT_DIRECTION`]. Set from an explicit `/rd` tag (or, once
    /// C17 lands, a resolved preset's stored direction) — `command_parser`
    /// only parses the tag/trigger; resolving a preset trigger into a
    /// direction is C17's job.
    pub direction: Option<String>,
    /// The model identifier to request (the active model from
    /// `llm-provider`/settings).
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    /// Quoted context to present to the model as reference material only
    /// — never rewritten, never echoed back in the reply. Set from an
    /// explicit `/q` tag, or a `quote_parser::split` heuristic match when
    /// there's no explicit tag; `None` folds no quote block into the
    /// request.
    pub quote: Option<String>,
    /// Target output language (e.g. `"de"`), from a `/lang` tag. `None`
    /// leaves the model to respond in the input's own language.
    pub lang: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            direction: None,
            model: String::new(),
            temperature: 0.3,
            max_tokens: None,
            quote: None,
            lang: None,
        }
    }
}

/// Assembles a system+user [`LlmRequest`] from `opts` and the user's
/// draft (`input`) — the text to actually refine, with any quoted context
/// already separated out by the caller (`command_parser` for an explicit
/// `/q`, `quote_parser::split` otherwise). Pure function of its inputs: no
/// command parsing, no I/O, so it's trivial to unit test and safe for the
/// orchestrator to call synchronously.
pub fn build(input: &str, opts: &BuildOptions) -> LlmRequest {
    let mut direction = opts
        .direction
        .as_deref()
        .unwrap_or(DEFAULT_DIRECTION)
        .to_string();
    if let Some(lang) = opts.lang.as_deref().filter(|l| !l.trim().is_empty()) {
        direction.push_str(&format!(
            " Render your reply in {lang}, regardless of the language of the input text."
        ));
    }

    let mut messages = vec![ChatMessage::system(direction)];
    if let Some(quote) = opts.quote.as_deref().filter(|q| !q.trim().is_empty()) {
        messages.push(ChatMessage::system(format!(
            "Quoted context from the conversation, for reference only — do not rewrite it or \
             include it in your reply:\n{quote}"
        )));
    }
    messages.push(ChatMessage::user(input));

    LlmRequest {
        messages,
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

    #[test]
    fn an_explicit_direction_overrides_the_default() {
        let opts = BuildOptions {
            direction: Some("make it concise".to_string()),
            ..Default::default()
        };

        let request = build("we was gonna ship fri", &opts);

        assert_eq!(request.messages[0].content, "make it concise");
        assert_eq!(
            request.messages.last().unwrap().content,
            "we was gonna ship fri"
        );
    }

    #[test]
    fn lang_appends_a_target_language_instruction_to_the_system_message() {
        let opts = BuildOptions {
            lang: Some("de".to_string()),
            ..Default::default()
        };

        let request = build("hi there", &opts);

        assert!(request.messages[0].content.starts_with(DEFAULT_DIRECTION));
        assert!(request.messages[0].content.contains("de"));
    }

    #[test]
    fn no_lang_means_no_language_instruction_is_appended() {
        let request = build("hi there", &BuildOptions::default());

        assert_eq!(request.messages[0].content, DEFAULT_DIRECTION);
    }

    #[test]
    fn quote_folds_in_as_a_separate_reference_only_message() {
        let opts = BuildOptions {
            quote: Some("On Mon, Alex wrote: any risk of slipping?".to_string()),
            ..Default::default()
        };

        let request = build("we're on track, shipping Monday", &opts);

        assert_eq!(
            request.messages.len(),
            3,
            "system direction + quote context + user draft"
        );
        let quote_message = &request.messages[1];
        assert_eq!(quote_message.role, "system");
        assert!(quote_message.content.contains("Alex wrote"));
        assert!(quote_message
            .content
            .to_lowercase()
            .contains("do not rewrite"));

        let user_message = request.messages.last().unwrap();
        assert_eq!(user_message.role, "user");
        assert_eq!(user_message.content, "we're on track, shipping Monday");
        assert!(
            !user_message.content.contains("Alex wrote"),
            "quoted context must not be duplicated into the text to rewrite"
        );
    }

    #[test]
    fn no_quote_means_no_extra_context_message_is_added() {
        let request = build("hi there", &BuildOptions::default());

        assert_eq!(
            request.messages.len(),
            2,
            "just system direction + user draft"
        );
    }

    #[test]
    fn an_empty_quote_is_treated_the_same_as_no_quote() {
        let opts = BuildOptions {
            quote: Some("   ".to_string()),
            ..Default::default()
        };

        let request = build("hi there", &opts);

        assert_eq!(request.messages.len(), 2);
    }

    #[test]
    fn direction_quote_and_lang_all_fold_in_together() {
        let opts = BuildOptions {
            direction: Some("keep it warm but concise".to_string()),
            quote: Some("Alex wrote: any risk of slipping?".to_string()),
            lang: Some("de".to_string()),
            model: "fake-model".to_string(),
            ..Default::default()
        };

        let request = build("we're good, shipping Monday", &opts);

        assert!(request.messages[0]
            .content
            .contains("keep it warm but concise"));
        assert!(request.messages[0].content.contains("de"));
        assert!(request.messages[1].content.contains("Alex wrote"));
        assert_eq!(
            request.messages.last().unwrap().content,
            "we're good, shipping Monday"
        );
        assert_eq!(request.model, "fake-model");
    }
}
