// Global hotkey register/unregister on top of `tauri-plugin-global-shortcut`.
//
// The plugin owns the actual OS-level registration; this module owns combo
// parsing, the current-hotkey bookkeeping, and conflict detection. Register
// the combo through the plugin's shortcut manager rather than intercepting
// raw keystrokes.
//
// The public command surface is `hotkey_set` (the same name A4 built and
// C6 later builds a rebind dialog on top of); A14's composition root
// (`lib.rs`) is responsible for wiring it into the Tauri builder's invoke
// handler and for managing `HotkeyState`. Everything else here
// (`apply_combo`, `ShortcutBackend`, `InMemoryRegistry`) is a
// private/internal helper.
//
// C2 adds persistence on top of A4's register-new-first/conflict-detection
// logic: `hotkey_set` writes a successful rebind to `SettingsStore` (see
// `persist_result`), and `register_startup` reads it back at launch (see
// `startup_combo`) so a rebind survives a restart instead of always coming
// back up on `DEFAULT_HOTKEY`. A conflict -- whether the combo is already
// claimed within this app, by another app, or by the OS, all of which
// `AppShortcutBackend::register` surfaces identically -- never persists and
// never touches the previously-registered combo (`apply_combo`'s
// register-new-first rollback), so the user is never left without a
// working hotkey.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/core_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use serde::Serialize;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Mutex;
use tauri_plugin_global_shortcut::Shortcut;

use crate::settings::SettingsStore;

/// Default global hotkey, matching the wireframe (`⌃⌥R`).
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+R";

/// Settings key the current hotkey combo persists under. `hotkey_set`
/// writes it on every successful rebind (see `persist_result`); the app's
/// startup (`register_startup`) reads it back so a rebind survives a
/// restart instead of always coming back up on `DEFAULT_HOTKEY`.
const HOTKEY_SETTING_KEY: &str = "hotkey_combo";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HotkeySetResult {
    pub ok: bool,
    pub conflict: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HotkeyError {
    InvalidCombo(String),
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::InvalidCombo(combo) => write!(f, "invalid hotkey combo: {combo}"),
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Parses a combo string (e.g. `"Ctrl+Alt+R"`) into a `Shortcut`.
fn parse(combo: &str) -> Result<Shortcut, HotkeyError> {
    Shortcut::from_str(combo).map_err(|_| HotkeyError::InvalidCombo(combo.to_string()))
}

/// Seam over the OS-level shortcut manager so register/unregister/conflict
/// logic is unit-testable without a running Tauri app.
pub trait ShortcutBackend {
    fn register(&self, shortcut: Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: Shortcut) -> Result<(), String>;
}

/// Tracks which shortcut ids are currently registered. Used both as the test
/// double for `ShortcutBackend` and, conceptually, as the shape the real
/// plugin-backed manager mirrors (register fails if already taken).
#[derive(Default)]
pub struct InMemoryRegistry {
    registered: Mutex<HashSet<u32>>,
}

impl ShortcutBackend for InMemoryRegistry {
    fn register(&self, shortcut: Shortcut) -> Result<(), String> {
        let mut reg = self.registered.lock().unwrap();
        if reg.contains(&shortcut.id()) {
            return Err("shortcut already registered".to_string());
        }
        reg.insert(shortcut.id());
        Ok(())
    }

    fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
        self.registered.lock().unwrap().remove(&shortcut.id());
        Ok(())
    }
}

/// Applies `combo` as the new hotkey against `backend`, replacing
/// `previous` if it differs. Returns `conflict: true` (without touching the
/// backend) when `combo` is already registered as a different shortcut.
/// Internal helper backing the `hotkey_set` command; kept private so the
/// public command surface stays just `hotkey_set`.
fn apply_combo<B: ShortcutBackend>(
    backend: &B,
    previous: Option<&str>,
    combo: &str,
) -> Result<HotkeySetResult, HotkeyError> {
    let shortcut = parse(combo)?;

    if let Some(prev) = previous {
        if prev == combo {
            // Re-saving the currently active combo is a no-op success.
            return Ok(HotkeySetResult {
                ok: true,
                conflict: false,
            });
        }
    }

    // Register the new combo first. Only once it succeeds do we release the
    // previous one, so a conflict on `combo` never leaves the user without a
    // working hotkey (and `state.current`/the backend never desync).
    match backend.register(shortcut) {
        Ok(()) => {
            if let Some(prev) = previous {
                let prev_shortcut = parse(prev)?;
                let _ = backend.unregister(prev_shortcut);
            }
            Ok(HotkeySetResult {
                ok: true,
                conflict: false,
            })
        }
        Err(_) => Ok(HotkeySetResult {
            ok: false,
            conflict: true,
        }),
    }
}

/// Real backend wrapping the app's `tauri-plugin-global-shortcut` manager.
pub struct AppShortcutBackend<R: tauri::Runtime>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> ShortcutBackend for AppShortcutBackend<R> {
    fn register(&self, shortcut: Shortcut) -> Result<(), String> {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        self.0
            .global_shortcut()
            .register(shortcut)
            .map_err(|e| e.to_string())
    }

    fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        self.0
            .global_shortcut()
            .unregister(shortcut)
            .map_err(|e| e.to_string())
    }
}

/// Holds the currently registered hotkey combo. Managed as Tauri state by
/// A14 and passed into the `hotkey_set` command.
///
/// `current` is `pub(crate)` rather than private so `lib.rs`'s global-
/// shortcut dispatch handler (`dispatch_hotkey`) can read which combo is
/// "live" when an OS-level shortcut event fires, without a getter method
/// that would otherwise be this struct's only reason to exist beyond a
/// plain field.
#[derive(Default)]
pub struct HotkeyState {
    pub(crate) current: Mutex<Option<String>>,
}

/// The combo `register_startup` should register at launch: a previously
/// persisted rebind (`hotkey_set`'s `HOTKEY_SETTING_KEY` write) if one
/// exists, otherwise `DEFAULT_HOTKEY`. Pure lookup, extracted out of
/// `register_startup` so it's unit-testable without a live shortcut
/// manager.
fn startup_combo(settings: &SettingsStore) -> String {
    settings
        .get(HOTKEY_SETTING_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

/// Registers the persisted hotkey combo (or `DEFAULT_HOTKEY`, see
/// `startup_combo`) at startup. Called by A14's setup (`lib.rs`); not
/// tested here since it needs a live app/OS shortcut manager -- the
/// persisted-vs-default combo choice itself is `startup_combo`'s test.
pub fn register_startup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &HotkeyState,
    settings: &SettingsStore,
) -> Result<HotkeySetResult, HotkeyError> {
    let combo = startup_combo(settings);
    let backend = AppShortcutBackend(app.clone());
    let result = apply_combo(&backend, None, &combo)?;
    if result.ok {
        *state.current.lock().unwrap() = Some(combo);
    }
    Ok(result)
}

/// Updates `state.current` and persists `combo` under `HOTKEY_SETTING_KEY`,
/// but only when `result.ok` -- a conflict must leave both the in-memory
/// and persisted combo untouched (the user keeps their working hotkey).
/// Split out of the `hotkey_set` command so the persistence behavior is
/// unit-testable without a Tauri command/App, mirroring `apply_combo`'s own
/// split.
fn persist_result(
    state: &HotkeyState,
    settings: &SettingsStore,
    combo: &str,
    result: &HotkeySetResult,
) -> Result<(), String> {
    if !result.ok {
        return Ok(());
    }
    *state.current.lock().unwrap() = Some(combo.to_string());
    settings
        .set(HOTKEY_SETTING_KEY, combo)
        .map_err(|e| e.to_string())
}

/// Tauri command: saves `combo` as the new global hotkey, unregistering the
/// previous one first, and persists it so a rebind survives a restart (see
/// `persist_result`/`register_startup`). Returns `conflict: true` when
/// `combo` is already registered elsewhere (by this app, another app, or
/// the OS -- the real `AppShortcutBackend::register` call surfaces all
/// three the same way) rather than erroring, so a rejected rebind never
/// leaves the app hotkey-less. Registered into the invoke handler by A14
/// (`lib.rs`), which manages `HotkeyState`; C6 later builds the rebind
/// dialog (S34) on top of this same command.
#[tauri::command]
pub fn hotkey_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, HotkeyState>,
    settings: tauri::State<'_, SettingsStore>,
    combo: String,
) -> Result<HotkeySetResult, String> {
    let backend = AppShortcutBackend(app);
    let previous = state.current.lock().unwrap().clone();
    let result = apply_combo(&backend, previous.as_deref(), &combo).map_err(|e| e.to_string())?;
    persist_result(&state, &settings, &combo, &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_combo() {
        assert!(parse(DEFAULT_HOTKEY).is_ok());
    }

    #[test]
    fn rejects_invalid_combo() {
        assert_eq!(
            parse("NotAKey"),
            Err(HotkeyError::InvalidCombo("NotAKey".to_string()))
        );
    }

    #[test]
    fn registers_a_fresh_combo() {
        let backend = InMemoryRegistry::default();
        let result = apply_combo(&backend, None, "Ctrl+Alt+R").unwrap();
        assert_eq!(
            result,
            HotkeySetResult {
                ok: true,
                conflict: false
            }
        );
    }

    #[test]
    fn reports_conflict_when_combo_already_registered() {
        let backend = InMemoryRegistry::default();
        // Simulate something else already owning this combo.
        backend.register(parse("Ctrl+Alt+T").unwrap()).unwrap();

        let result = apply_combo(&backend, None, "Ctrl+Alt+T").unwrap();
        assert_eq!(
            result,
            HotkeySetResult {
                ok: false,
                conflict: true
            }
        );
    }

    #[test]
    fn resaving_the_same_combo_is_a_noop_success() {
        let backend = InMemoryRegistry::default();
        apply_combo(&backend, None, "Ctrl+Alt+R").unwrap();

        let result = apply_combo(&backend, Some("Ctrl+Alt+R"), "Ctrl+Alt+R").unwrap();
        assert_eq!(
            result,
            HotkeySetResult {
                ok: true,
                conflict: false
            }
        );
    }

    #[test]
    fn switching_combo_unregisters_the_previous_one() {
        let backend = InMemoryRegistry::default();
        apply_combo(&backend, None, "Ctrl+Alt+R").unwrap();

        let result = apply_combo(&backend, Some("Ctrl+Alt+R"), "Ctrl+Alt+S").unwrap();
        assert_eq!(
            result,
            HotkeySetResult {
                ok: true,
                conflict: false
            }
        );
        // The previous combo was freed, so it can be registered again.
        assert!(backend.register(parse("Ctrl+Alt+R").unwrap()).is_ok());
    }

    #[test]
    fn rebind_conflict_leaves_previous_combo_registered() {
        let backend = InMemoryRegistry::default();
        // Something else already owns this combo.
        backend.register(parse("Ctrl+Alt+T").unwrap()).unwrap();
        // The user's currently active combo.
        apply_combo(&backend, None, "Ctrl+Alt+R").unwrap();

        let result = apply_combo(&backend, Some("Ctrl+Alt+R"), "Ctrl+Alt+T").unwrap();
        assert_eq!(
            result,
            HotkeySetResult {
                ok: false,
                conflict: true
            }
        );
        // The previous combo must still be registered: trying to register it
        // again should fail because it's still held.
        assert!(backend.register(parse("Ctrl+Alt+R").unwrap()).is_err());
    }

    #[test]
    fn invalid_combo_returns_error() {
        let backend = InMemoryRegistry::default();
        assert_eq!(
            apply_combo(&backend, None, "NotAKey"),
            Err(HotkeyError::InvalidCombo("NotAKey".to_string()))
        );
    }

    // ---- persist_result / startup_combo (C2: rebind persistence) ----

    #[test]
    fn persist_result_saves_the_combo_on_success() {
        let hotkey_state = HotkeyState::default();
        let settings = SettingsStore::open_in_memory().unwrap();
        let result = HotkeySetResult {
            ok: true,
            conflict: false,
        };

        persist_result(&hotkey_state, &settings, "Ctrl+Alt+S", &result).unwrap();

        assert_eq!(
            *hotkey_state.current.lock().unwrap(),
            Some("Ctrl+Alt+S".to_string())
        );
        assert_eq!(
            settings.get(HOTKEY_SETTING_KEY).unwrap(),
            Some("Ctrl+Alt+S".to_string())
        );
    }

    #[test]
    fn persist_result_does_nothing_on_conflict() {
        let hotkey_state = HotkeyState::default();
        *hotkey_state.current.lock().unwrap() = Some(DEFAULT_HOTKEY.to_string());
        let settings = SettingsStore::open_in_memory().unwrap();
        let result = HotkeySetResult {
            ok: false,
            conflict: true,
        };

        persist_result(&hotkey_state, &settings, "Ctrl+Alt+T", &result).unwrap();

        // Neither the in-memory state nor the persisted combo changed --
        // the user's working hotkey survives a rejected rebind attempt.
        assert_eq!(
            *hotkey_state.current.lock().unwrap(),
            Some(DEFAULT_HOTKEY.to_string())
        );
        assert_eq!(settings.get(HOTKEY_SETTING_KEY).unwrap(), None);
    }

    #[test]
    fn startup_combo_falls_back_to_the_default_when_nothing_is_persisted() {
        let settings = SettingsStore::open_in_memory().unwrap();
        assert_eq!(startup_combo(&settings), DEFAULT_HOTKEY);
    }

    #[test]
    fn startup_combo_returns_the_persisted_combo_when_present() {
        let settings = SettingsStore::open_in_memory().unwrap();
        settings.set(HOTKEY_SETTING_KEY, "Ctrl+Alt+S").unwrap();
        assert_eq!(startup_combo(&settings), "Ctrl+Alt+S");
    }
}
