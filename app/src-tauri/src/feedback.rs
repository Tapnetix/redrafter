// Feedback cues for an in-flight refine: the menu-bar spinner, a
// near-cursor HUD, and a completion sound (SC13/S35, wireframes/behavior.html
// "Progress feedback" -- `feedback.spinner`/`feedback.hud`/`feedback.sound`
// settings keys, `settings.rs`).
//
// `on_refine_start`/`on_refine_done` are the two hooks `run_refine`
// (`lib.rs`, wired by C17) will call right before the orchestrator starts
// and right after it finishes; each returns exactly the cues the user has
// switched on, so the frontend spinner/HUD only ever hear about a cue that
// is actually enabled. Registering `feedback_config_get`/`feedback_config_set`
// in the invoke handler + ACL is also C17's job (this task only builds the
// commands and their pure inner logic).
//
// Playback of the completion sound itself is a thin, `cfg`-gated,
// best-effort side effect of `on_refine_done` -- it never changes which
// cues are *reported*, so the gating logic above stays fully unit-testable
// without a real sound device. macOS uses `afplay` against a system sound;
// other platforms are a later phase.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/feedback_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use serde::{Deserialize, Serialize};

use crate::settings::SettingsStore;

/// A single in-flight/completion feedback cue, matching the Behavior
/// screen's three toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackCue {
    /// The menu-bar tray icon's spinner state.
    Spinner,
    /// The floating pill HUD near the cursor.
    Hud,
    /// The completion chime.
    Sound,
}

/// Which feedback cues are currently enabled -- the Tauri-serializable
/// shape `feedback_config_get`/`feedback_config_set` round-trip to the
/// frontend, and `on_refine_start`/`on_refine_done` gate their cues on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackConfig {
    pub spinner: bool,
    pub hud: bool,
    pub sound: bool,
}

impl FeedbackConfig {
    /// Reads the three toggles from `settings` (`SettingsStore`'s typed
    /// `feedback_*_enabled` accessors), each defaulting to the Behavior
    /// screen's own default selection (spinner/sound on, HUD off).
    pub fn from_settings(settings: &SettingsStore) -> Result<Self, String> {
        Ok(Self {
            spinner: settings
                .feedback_spinner_enabled()
                .map_err(|e| e.to_string())?,
            hud: settings.feedback_hud_enabled().map_err(|e| e.to_string())?,
            sound: settings
                .feedback_sound_enabled()
                .map_err(|e| e.to_string())?,
        })
    }

    /// Persists all three toggles to `settings` in one call.
    pub fn persist(&self, settings: &SettingsStore) -> Result<(), String> {
        settings
            .set_feedback_spinner_enabled(self.spinner)
            .map_err(|e| e.to_string())?;
        settings
            .set_feedback_hud_enabled(self.hud)
            .map_err(|e| e.to_string())?;
        settings
            .set_feedback_sound_enabled(self.sound)
            .map_err(|e| e.to_string())
    }

    /// The cues to show while a refine is in flight: the spinner and/or
    /// HUD, whichever are enabled. There's nothing to play a sound for
    /// yet, so `sound` never appears here (`on_refine_done` is where it's
    /// added).
    fn in_flight_cues(&self) -> Vec<FeedbackCue> {
        let mut cues = Vec::new();
        if self.spinner {
            cues.push(FeedbackCue::Spinner);
        }
        if self.hud {
            cues.push(FeedbackCue::Hud);
        }
        cues
    }
}

/// Cues to fire when a refine begins: the spinner/HUD, each gated on its
/// own toggle in `settings`. Called by `run_refine` (`lib.rs`, wired by
/// C17) right before the orchestrator starts; the frontend clears these on
/// the matching `on_refine_done` cue (or on error/cancel).
pub fn on_refine_start(settings: &SettingsStore) -> Result<Vec<FeedbackCue>, String> {
    Ok(FeedbackConfig::from_settings(settings)?.in_flight_cues())
}

/// Cues to fire when a refine completes: clears the same in-flight cues
/// (spinner/HUD) and, if enabled, the completion sound -- which this also
/// best-effort plays via [`play_completion_sound`].
pub fn on_refine_done(settings: &SettingsStore) -> Result<Vec<FeedbackCue>, String> {
    let config = FeedbackConfig::from_settings(settings)?;
    let mut cues = config.in_flight_cues();
    if config.sound {
        cues.push(FeedbackCue::Sound);
        play_completion_sound();
    }
    Ok(cues)
}

/// Best-effort completion-sound playback: macOS-only for now (`afplay`
/// against a system sound), a no-op everywhere else. Never affects
/// [`on_refine_done`]'s returned cues -- a missing/failed player just means
/// silence, not an error, which is why this returns nothing and swallows
/// the spawn result.
fn play_completion_sound() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Pop.aiff")
            .spawn();
    }
}

/// Tauri command: reads the persisted feedback config. Registered by C17
/// (`app/src-tauri/src/lib.rs`), which manages a `SettingsStore` as state.
#[tauri::command]
pub fn feedback_config_get(
    state: tauri::State<'_, SettingsStore>,
) -> Result<FeedbackConfig, String> {
    FeedbackConfig::from_settings(&state)
}

/// Tauri command: persists the feedback config. Registered by C17
/// (`app/src-tauri/src/lib.rs`), which manages a `SettingsStore` as state.
#[tauri::command]
pub fn feedback_config_set(
    state: tauri::State<'_, SettingsStore>,
    config: FeedbackConfig,
) -> Result<(), String> {
    config.persist(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(spinner: bool, hud: bool, sound: bool) -> SettingsStore {
        let store = SettingsStore::open_in_memory().expect("in-memory store");
        store.set_feedback_spinner_enabled(spinner).unwrap();
        store.set_feedback_hud_enabled(hud).unwrap();
        store.set_feedback_sound_enabled(sound).unwrap();
        store
    }

    #[test]
    fn from_settings_reads_defaults_when_unset() {
        let store = SettingsStore::open_in_memory().unwrap();
        assert_eq!(
            FeedbackConfig::from_settings(&store).unwrap(),
            FeedbackConfig {
                spinner: true,
                hud: false,
                sound: true,
            }
        );
    }

    #[test]
    fn in_flight_cues_includes_spinner_only_when_enabled_alone() {
        let config = FeedbackConfig {
            spinner: true,
            hud: false,
            sound: false,
        };
        assert_eq!(config.in_flight_cues(), vec![FeedbackCue::Spinner]);
    }

    #[test]
    fn in_flight_cues_includes_hud_only_when_enabled_alone() {
        let config = FeedbackConfig {
            spinner: false,
            hud: true,
            sound: false,
        };
        assert_eq!(config.in_flight_cues(), vec![FeedbackCue::Hud]);
    }

    #[test]
    fn on_refine_start_reflects_settings_toggles() {
        let store = store_with(false, true, true);
        assert_eq!(on_refine_start(&store).unwrap(), vec![FeedbackCue::Hud]);
    }

    #[test]
    fn on_refine_done_reflects_all_settings_toggles() {
        let store = store_with(false, false, true);
        assert_eq!(on_refine_done(&store).unwrap(), vec![FeedbackCue::Sound]);
    }

    #[test]
    fn on_refine_done_fires_nothing_when_all_toggles_are_off() {
        let store = store_with(false, false, false);
        assert_eq!(on_refine_done(&store).unwrap(), Vec::new());
    }
}
