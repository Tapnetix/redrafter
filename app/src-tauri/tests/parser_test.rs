//! Exercises `command_parser`, `quote_parser`, and the B4 extensions to
//! `prompt_builder` before they're wired into `lib.rs`'s module tree
//! (command-handler wiring into the orchestrator is B5/B23).
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/command_parser.rs"]` — see
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

// Wrapped in a module whose name contains "parser" so the plan's
// verification filter (`cargo nextest run -p redrafter command_parser
// quote_parser prompt_builder parser`) discovers these tests by substring
// match on the qualified test name.
mod parser_integration_tests {
    use super::command_parser;
    use super::prompt_builder::{self, BuildOptions};
    use super::quote_parser;

    /// Mirrors the orchestrator wiring B5 will add: parse the selection's
    /// commands, fall back to heuristic quote detection when there's no
    /// explicit `/q`, and hand the result to `prompt_builder::build`.
    fn build_request(selection: &str, model: &str) -> llm_provider::LlmRequest {
        let parsed = command_parser::parse(selection);

        let (quote, draft) = match parsed.quote {
            Some(q) => (Some(q), parsed.message),
            None => quote_parser::split(&parsed.message),
        };

        let opts = BuildOptions {
            direction: parsed.direction,
            model: model.to_string(),
            quote,
            lang: parsed.lang,
            ..Default::default()
        };
        prompt_builder::build(&draft, &opts)
    }

    #[test]
    fn s2_inline_direction_and_message_are_split_and_tags_stripped() {
        let selection = "/rd make it concise /m we was gonna ship fri";
        let request = build_request(selection, "fake-model");

        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[0].content, "make it concise");

        let user = request.messages.last().unwrap();
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "we was gonna ship fri");
        assert!(!request.messages.iter().any(|m| m.content.contains("/rd")));
        assert!(!request.messages.iter().any(|m| m.content.contains("/m")));
    }

    #[test]
    fn s3_quoted_reply_block_is_context_and_draft_is_polished() {
        let selection = "> On Mon, Alex wrote: are we still on track for Q3?\n\
                          /rd read the quoted thread and refine my answer, keep it warm but concise \
                          /m We're good with the Q3 release plan, no delays, shipping by Monday";
        let request = build_request(selection, "fake-model");

        let user = request.messages.last().unwrap();
        assert_eq!(
            user.content,
            "We're good with the Q3 release plan, no delays, shipping by Monday"
        );
        assert!(
            !user.content.contains("Alex wrote"),
            "the quote must not end up in the text to rewrite"
        );

        let quote_message = request
            .messages
            .iter()
            .find(|m| m.content.contains("Alex wrote"))
            .expect("quoted context should be folded in as a system message");
        assert!(
            quote_message.content.contains("reference only")
                || quote_message
                    .content
                    .to_lowercase()
                    .contains("do not rewrite")
        );
    }

    #[test]
    fn s4_lang_tag_produces_a_target_language_instruction() {
        let selection = "/lang de /m Wir sind auf Kurs";
        let request = build_request(selection, "fake-model");

        assert!(request.messages[0].content.contains("de"));
        let user = request.messages.last().unwrap();
        assert_eq!(user.content, "Wir sind auf Kurs");
    }

    #[test]
    fn explicit_q_overrides_heuristic_quote_detection() {
        let selection = "/q the actual quote here /m my draft text";
        let request = build_request(selection, "fake-model");

        let user = request.messages.last().unwrap();
        assert_eq!(user.content, "my draft text");
        let quote_message = request
            .messages
            .iter()
            .find(|m| m.content.contains("the actual quote here"))
            .expect("explicit /q content should be folded in as context");
        assert!(quote_message.role == "system");
    }

    #[test]
    fn no_commands_default_refine_still_works() {
        let request = build_request("just fix this up please", "fake-model");

        assert_eq!(
            request.messages[0].content,
            prompt_builder::DEFAULT_DIRECTION
        );
        assert_eq!(
            request.messages.last().unwrap().content,
            "just fix this up please"
        );
    }
}
