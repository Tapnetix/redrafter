//! Phase C command-wiring boundary tests (C17): the seams this wire-up task
//! actually closes — resolving a parsed `/preset` trigger (B4's
//! `command_parser`) through the preset store (C3/C3b's `presets`) and
//! folding its direction/model/language/inject overrides into the built
//! refine request (`orchestrator::resolve_prompt` -> `prompt_builder::build`),
//! with an explicit `/rd` still winning over the preset's own direction.
//!
//! These exercise the exact `resolve_prompt` the live pipeline runs (the
//! `orchestrator` module compiled below is the same source `lib.rs`'s
//! `execute_refine` calls), against a real in-memory `PresetStore` — so a
//! green run here proves an inline preset trigger genuinely changes the
//! refine, the acceptance bar this task is measured against.
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

// Wrapped in a module whose name contains "commands_c" so the plan's
// verification filter (`cargo nextest run -p redrafter commands_c preset`)
// discovers these tests by substring match on the qualified test name.
mod commands_c_preset_resolution_tests {
    use super::orchestrator::resolve_prompt;
    use super::presets::PresetStore;
    use super::prompt_builder::{self, BuildOptions};

    fn store_with_reply_de() -> PresetStore {
        let store = PresetStore::open_in_memory().expect("failed to open in-memory preset store");
        store
            .save(
                "reply-de",
                "Reply in German, warm but concise.",
                Some("claude-opus-4-6"),
                Some("de"),
                None,
                &[],
            )
            .expect("failed to save the reply-de preset");
        store
    }

    #[test]
    fn a_triggered_presets_direction_and_model_reach_the_built_request() {
        let presets = store_with_reply_de();
        let (draft, resolved, _mode) = resolve_prompt(
            "/reply-de thanks, sounds good — talk tomorrow",
            &BuildOptions {
                model: "gpt-5.1".to_string(),
                ..BuildOptions::default()
            },
            Some(&presets),
        );

        // The trigger's trailing text is the draft to refine...
        assert_eq!(draft, "thanks, sounds good — talk tomorrow");

        let request = prompt_builder::build(&draft, &resolved);
        // ...the preset's direction is the system prompt...
        assert!(request.messages[0]
            .content
            .starts_with("Reply in German, warm but concise."));
        // ...its `lang` override appends the target-language instruction...
        assert!(request.messages[0].content.contains("de"));
        // ...and its `model` override is what gets requested (over the
        // caller's `gpt-5.1`).
        assert_eq!(request.model, "claude-opus-4-6");
    }

    #[test]
    fn an_explicit_rd_still_wins_over_the_triggered_presets_direction() {
        let presets = store_with_reply_de();
        let (_draft, resolved, _mode) = resolve_prompt(
            "/reply-de /rd keep it in English, one line",
            &BuildOptions::default(),
            Some(&presets),
        );

        let request = prompt_builder::build("hi", &resolved);
        // The explicit `/rd` direction is the system prompt (over the
        // preset's own direction)...
        assert!(request.messages[0]
            .content
            .starts_with("keep it in English, one line"));
        // ...but the preset's *model* and *lang* overrides still apply --
        // `/rd` overrides only the direction, not the rest of the preset.
        assert!(request.messages[0].content.contains("de"));
        assert_eq!(request.model, "claude-opus-4-6");
    }

    #[test]
    fn an_unknown_trigger_leaves_the_request_on_the_defaults() {
        let presets = store_with_reply_de();
        let (draft, resolved, mode) = resolve_prompt(
            "/no-such-preset just polish this",
            &BuildOptions {
                model: "gpt-5.1".to_string(),
                ..BuildOptions::default()
            },
            Some(&presets),
        );

        assert_eq!(draft, "just polish this");
        assert_eq!(resolved.model, "gpt-5.1", "no preset -> caller's model stands");
        assert_eq!(resolved.direction, None, "no preset -> no direction override");
        assert_eq!(mode, None);
    }
}
