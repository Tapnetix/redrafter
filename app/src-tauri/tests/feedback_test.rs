//! Compiles `feedback.rs` (C1) ahead of C17 wiring it into `lib.rs`'s
//! module tree, and exercises `on_refine_start`/`on_refine_done` against a
//! real (in-memory) `SettingsStore` -- confirming each cue (menu-bar
//! spinner, cursor HUD, completion sound) fires only when its own toggle is
//! on, independent of the other two.
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/feedback.rs"]` -- see
//! `tests/core_test.rs` for why: a relative `#[path]` from a file under
//! `tests/` embeds a literal, unnormalized `tests/../src/...` path in debug
//! info, which `cargo llvm-cov`'s default ignore rule for `tests/` silently
//! filters out -- zeroing coverage for this task's real production code. The
//! absolute path here resolves to plain `.../src/*.rs`, so coverage
//! attributes correctly.

mod settings {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/settings.rs"));
}
mod feedback {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/feedback.rs"));
}

// Wrapped in a module whose name contains "feedback" so the plan's
// verification filter (`cargo nextest run -p redrafter feedback`)
// discovers these tests by substring match on the qualified test name --
// mirroring how `feedback`'s own `#[cfg(test)] mod tests` (embedded above
// via `include!`) is discoverable through its module path already
// containing "feedback".
mod feedback_integration_tests {
    use super::feedback::{on_refine_done, on_refine_start, FeedbackConfig, FeedbackCue};
    use super::settings::SettingsStore;

    fn store_with(spinner: bool, hud: bool, sound: bool) -> SettingsStore {
        let store = SettingsStore::open_in_memory().expect("in-memory store");
        store.set_feedback_spinner_enabled(spinner).unwrap();
        store.set_feedback_hud_enabled(hud).unwrap();
        store.set_feedback_sound_enabled(sound).unwrap();
        store
    }

    #[test]
    fn on_refine_start_fires_only_the_enabled_in_flight_cues() {
        let store = store_with(true, true, true);
        let cues = on_refine_start(&store).unwrap();
        assert_eq!(cues, vec![FeedbackCue::Spinner, FeedbackCue::Hud]);
    }

    #[test]
    fn on_refine_start_never_fires_sound() {
        let store = store_with(true, true, true);
        let cues = on_refine_start(&store).unwrap();
        assert!(!cues.contains(&FeedbackCue::Sound));
    }

    #[test]
    fn on_refine_start_fires_nothing_when_all_toggles_are_off() {
        let store = store_with(false, false, false);
        assert_eq!(on_refine_start(&store).unwrap(), Vec::new());
    }

    #[test]
    fn on_refine_done_fires_all_three_cues_when_all_enabled() {
        let store = store_with(true, true, true);
        let cues = on_refine_done(&store).unwrap();
        assert_eq!(
            cues,
            vec![FeedbackCue::Spinner, FeedbackCue::Hud, FeedbackCue::Sound]
        );
    }

    #[test]
    fn on_refine_done_omits_sound_when_disabled() {
        let store = store_with(true, true, false);
        let cues = on_refine_done(&store).unwrap();
        assert!(!cues.contains(&FeedbackCue::Sound));
    }

    #[test]
    fn on_refine_done_omits_spinner_when_disabled() {
        let store = store_with(false, true, true);
        let cues = on_refine_done(&store).unwrap();
        assert!(!cues.contains(&FeedbackCue::Spinner));
        assert!(cues.contains(&FeedbackCue::Hud));
        assert!(cues.contains(&FeedbackCue::Sound));
    }

    #[test]
    fn on_refine_done_omits_hud_when_disabled() {
        let store = store_with(true, false, true);
        let cues = on_refine_done(&store).unwrap();
        assert!(!cues.contains(&FeedbackCue::Hud));
    }

    #[test]
    fn feedback_config_round_trips_through_settings() {
        let store = SettingsStore::open_in_memory().expect("in-memory store");
        let config = FeedbackConfig {
            spinner: false,
            hud: true,
            sound: false,
        };
        config.persist(&store).unwrap();
        assert_eq!(FeedbackConfig::from_settings(&store).unwrap(), config);
    }
}
