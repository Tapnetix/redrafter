// SQLite-backed key-value settings store.
//
// Every other backend module reads persisted app state (theme,
// launch-at-login, active model, hotkey combo, etc.) through this single
// store rather than keeping ad-hoc state of its own. The table is created
// on first run and the connection runs in WAL mode for safe concurrent
// reads while a write is in flight.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/core_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// Settings key for the menu-bar spinner feedback cue toggle (`feedback.rs`'s
/// `on_refine_start`/`on_refine_done`, Behavior screen's
/// `data-testid="feedback-spinner"`). Unset defaults to enabled, matching
/// the wireframe's default (checked) selection.
const FEEDBACK_SPINNER_KEY: &str = "feedback.spinner";
/// Settings key for the cursor HUD feedback cue toggle (Behavior screen's
/// `data-testid="feedback-hud"`). Unset defaults to disabled, matching the
/// wireframe's default (unchecked) selection.
const FEEDBACK_HUD_KEY: &str = "feedback.hud";
/// Settings key for the completion-sound feedback cue toggle (Behavior
/// screen's `data-testid="feedback-sound"`). Unset defaults to enabled,
/// matching the wireframe's default (checked) selection.
const FEEDBACK_SOUND_KEY: &str = "feedback.sound";

pub struct SettingsStore {
    conn: Mutex<Connection>,
}

impl SettingsStore {
    /// Opens a file-backed SQLite database at the given path, creating the
    /// settings table if this is the first run.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Opens an in-memory SQLite database (used by tests and defaults).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    /// Returns the value for the given key, or `None` if not present.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Returns the value for the given key, or `default` if not present.
    pub fn get_or(&self, key: &str, default: &str) -> Result<String> {
        Ok(self.get(key)?.unwrap_or_else(|| default.to_string()))
    }

    /// Upserts a key-value pair into the settings table.
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Reads a boolean-valued setting, defaulting when unset or unrecognized
    /// (mirrors `lib.rs`'s `is_paused`, which predates these typed helpers).
    fn get_bool(&self, key: &str, default: bool) -> Result<bool> {
        Ok(match self.get(key)?.as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        })
    }

    /// Persists a boolean-valued setting as the literal `"true"`/`"false"`
    /// string `get_bool` reads back.
    fn set_bool(&self, key: &str, value: bool) -> Result<()> {
        self.set(key, if value { "true" } else { "false" })
    }

    /// Whether the menu-bar spinner feedback cue is enabled (`feedback.rs`);
    /// defaults to on, matching the Behavior screen's default selection.
    pub fn feedback_spinner_enabled(&self) -> Result<bool> {
        self.get_bool(FEEDBACK_SPINNER_KEY, true)
    }

    /// Persists the menu-bar spinner feedback cue toggle.
    pub fn set_feedback_spinner_enabled(&self, enabled: bool) -> Result<()> {
        self.set_bool(FEEDBACK_SPINNER_KEY, enabled)
    }

    /// Whether the cursor HUD feedback cue is enabled (`feedback.rs`);
    /// defaults to off, matching the Behavior screen's default selection.
    pub fn feedback_hud_enabled(&self) -> Result<bool> {
        self.get_bool(FEEDBACK_HUD_KEY, false)
    }

    /// Persists the cursor HUD feedback cue toggle.
    pub fn set_feedback_hud_enabled(&self, enabled: bool) -> Result<()> {
        self.set_bool(FEEDBACK_HUD_KEY, enabled)
    }

    /// Whether the completion-sound feedback cue is enabled (`feedback.rs`);
    /// defaults to on, matching the Behavior screen's default selection.
    pub fn feedback_sound_enabled(&self) -> Result<bool> {
        self.get_bool(FEEDBACK_SOUND_KEY, true)
    }

    /// Persists the completion-sound feedback cue toggle.
    pub fn set_feedback_sound_enabled(&self, enabled: bool) -> Result<()> {
        self.set_bool(FEEDBACK_SOUND_KEY, enabled)
    }
}

/// Tauri command: reads a settings value by key. Registered by A14
/// (`app/src-tauri/src/lib.rs`), which manages a `SettingsStore` as state.
#[tauri::command]
pub fn settings_get(
    state: tauri::State<'_, SettingsStore>,
    key: String,
) -> Result<Option<String>, String> {
    state.get(&key).map_err(|e| e.to_string())
}

/// Tauri command: upserts a settings value by key. Registered by A14
/// (`app/src-tauri/src/lib.rs`), which manages a `SettingsStore` as state.
#[tauri::command]
pub fn settings_set(
    state: tauri::State<'_, SettingsStore>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.set(&key, &value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_store() -> SettingsStore {
        SettingsStore::open_in_memory().expect("failed to open in-memory store")
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = new_store();
        assert_eq!(store.get("nonexistent.key").unwrap(), None);
    }

    #[test]
    fn set_then_get_returns_value() {
        let store = new_store();
        store.set("foo", "bar").unwrap();
        assert_eq!(store.get("foo").unwrap(), Some("bar".to_string()));
    }

    #[test]
    fn set_overwrites_existing_value() {
        let store = new_store();
        store.set("key", "first").unwrap();
        store.set("key", "second").unwrap();
        assert_eq!(store.get("key").unwrap(), Some("second".to_string()));
    }

    #[test]
    fn get_or_falls_back_to_default_when_missing() {
        let store = new_store();
        assert_eq!(store.get_or("missing", "fallback").unwrap(), "fallback");
    }

    #[test]
    fn get_or_returns_stored_value_when_present() {
        let store = new_store();
        store.set("present", "value").unwrap();
        assert_eq!(store.get_or("present", "fallback").unwrap(), "value");
    }

    // ---- feedback cue toggles (C1) ----

    #[test]
    fn feedback_spinner_defaults_to_enabled() {
        let store = new_store();
        assert!(store.feedback_spinner_enabled().unwrap());
    }

    #[test]
    fn feedback_hud_defaults_to_disabled() {
        let store = new_store();
        assert!(!store.feedback_hud_enabled().unwrap());
    }

    #[test]
    fn feedback_sound_defaults_to_enabled() {
        let store = new_store();
        assert!(store.feedback_sound_enabled().unwrap());
    }

    #[test]
    fn feedback_spinner_persists_disabled() {
        let store = new_store();
        store.set_feedback_spinner_enabled(false).unwrap();
        assert!(!store.feedback_spinner_enabled().unwrap());
    }

    #[test]
    fn feedback_hud_persists_enabled() {
        let store = new_store();
        store.set_feedback_hud_enabled(true).unwrap();
        assert!(store.feedback_hud_enabled().unwrap());
    }

    #[test]
    fn feedback_sound_persists_disabled() {
        let store = new_store();
        store.set_feedback_sound_enabled(false).unwrap();
        assert!(!store.feedback_sound_enabled().unwrap());
    }
}
