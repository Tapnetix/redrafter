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

/// Appended to every refine, whatever direction is in force: the model must
/// return the rewrite and nothing else, wrapped in tags we can read back.
pub const OUTPUT_CONTRACT: &str = "Return ONLY the rewritten text, wrapped in \
<refined></refined> tags, like: <refined>the rewritten text</refined>. Put nothing outside the \
tags. Do not explain what you changed, do not add notes, headings, or commentary, and do not \
wrap the text in quotes or code fences. If the input needs no changes, return it unchanged \
inside the tags.";

/// How a selection's quoted context is handled when there's no explicit
/// `/q` tag — the Behavior screen's "When the selection has a quote"
/// setting (`behavior.quote_mode`), threaded in by the command layer
/// (`lib.rs`) and consumed by `orchestrator::resolve_prompt`. An explicit
/// `/q` tag always wins regardless of this mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuoteMode {
    /// "Answer only" (`answer`): heuristically split any detected quoted
    /// context out of the selection and refine only the user's own words,
    /// folding the quote in as reference-only context.
    AnswerOnly,
    /// "Answer + quote" (`answer_quote`): don't auto-strip — refine the
    /// whole selection as one draft. The Behavior screen's default.
    #[default]
    IncludeQuote,
    /// "Let /rd decide" (`rd`): don't auto-strip either; leave any quote
    /// handling to the direction/`/rd` instruction rather than the
    /// heuristic splitter.
    LetDirectionDecide,
}

/// Options controlling how a refine prompt is assembled.
///
/// `Default` is derived: every field's default is its type's, now that
/// `temperature` defaults to `None` (unset) rather than a fixed 0.3.
#[derive(Debug, Clone, Default)]
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
    /// Sampling temperature. `None` (the default) sends none at all —
    /// Anthropic deprecated the parameter on its newer models and rejects the
    /// request outright, which broke every refine on those models.
    pub temperature: Option<f32>,
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
    /// How to treat a heuristically-detected quote when there's no explicit
    /// `/q` tag (Behavior/`behavior.quote_mode`). Only read by
    /// `orchestrator::resolve_prompt`; ignored by [`build`] itself, which
    /// only cares about the already-resolved [`BuildOptions::quote`].
    pub quote_mode: QuoteMode,
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

    // The output contract is appended to *whatever* direction is in force,
    // rather than living inside DEFAULT_DIRECTION. A `/rd` instruction or a
    // preset replaces the direction wholesale, which used to take the only
    // "reply with just the text" wording with it — so a custom or friendly
    // direction was markedly more likely to come back with an explanation
    // attached, and that explanation was pasted into the user's document.
    //
    // A delimiter is honoured far more reliably than a plain request for no
    // commentary; `response_cleaner` reads what is between the tags, and falls
    // back to trimming if a model ignores them.
    let mut messages = vec![
        ChatMessage::system(direction),
        ChatMessage::system(OUTPUT_CONTRACT.to_string()),
    ];
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
            4,
            "system direction + output contract + quote context + user draft"
        );
        let quote_message = request
            .messages
            .iter()
            .find(|m| m.content.contains("Alex wrote"))
            .expect("the quote should be its own system message");
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
            3,
            "system direction + output contract + user draft"
        );
    }

    #[test]
    fn an_empty_quote_is_treated_the_same_as_no_quote() {
        let opts = BuildOptions {
            quote: Some("   ".to_string()),
            ..Default::default()
        };

        let request = build("hi there", &opts);

        assert_eq!(request.messages.len(), 3);
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
        assert!(request
            .messages
            .iter()
            .any(|m| m.content.contains("Alex wrote")));
        assert_eq!(
            request.messages.last().unwrap().content,
            "we're good, shipping Monday"
        );
        assert_eq!(request.model, "fake-model");
    }
}

#[cfg(test)]
mod output_contract_tests {
    use super::*;

    fn system_text(request: &LlmRequest) -> String {
        request
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_refine_asks_for_the_rewrite_in_tags() {
        let request = build("hi", &BuildOptions::default());
        assert!(system_text(&request).contains("<refined></refined>"));
    }

    /// The regression this exists for: a `/rd` instruction or a preset
    /// replaces the direction wholesale, which used to discard the only
    /// "reply with just the text" wording — so custom directions were the
    /// ones most likely to come back with an explanation attached.
    #[test]
    fn a_custom_direction_still_gets_the_output_contract() {
        let opts = BuildOptions {
            direction: Some("make it friendly".to_string()),
            ..Default::default()
        };
        let request = build("hi", &opts);
        let system = system_text(&request);

        assert!(system.contains("make it friendly"), "the direction survives");
        assert!(system.contains("<refined></refined>"), "and so does the contract");
        assert!(system.to_lowercase().contains("do not explain"));
    }

    #[test]
    fn the_contract_does_not_displace_the_draft() {
        let request = build("the draft text", &BuildOptions::default());
        let user = request.messages.last().unwrap();
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "the draft text");
    }
}
