// Global hotkey register/unregister on top of `tauri-plugin-global-shortcut`.
//
// The plugin owns the actual OS-level registration; this module owns combo
// parsing, the current-hotkey bookkeeping, and conflict detection. Register
// the combo through the plugin's shortcut manager rather than intercepting
// raw keystrokes.
//
// The public command surface is `hotkey_set` (the same name C2/C6 later
// extend for rebinding); A14's composition root (`lib.rs`) is responsible
// for wiring it into the Tauri builder's invoke handler and for managing
// `HotkeyState`. Everything else here (`apply_combo`, `ShortcutBackend`,
// `InMemoryRegistry`) is a private/internal helper.
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

/// Default global hotkey, matching the wireframe (`⌃⌥R`).
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+R";

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
        let prev_shortcut = parse(prev)?;
        let _ = backend.unregister(prev_shortcut);
    }

    match backend.register(shortcut) {
        Ok(()) => Ok(HotkeySetResult {
            ok: true,
            conflict: false,
        }),
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
#[derive(Default)]
pub struct HotkeyState {
    current: Mutex<Option<String>>,
}

/// Registers `DEFAULT_HOTKEY` at startup. Called by A14's setup, not tested
/// here since it needs a live app/OS shortcut manager.
pub fn register_default<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &HotkeyState,
) -> Result<HotkeySetResult, HotkeyError> {
    let backend = AppShortcutBackend(app.clone());
    let result = apply_combo(&backend, None, DEFAULT_HOTKEY)?;
    if result.ok {
        *state.current.lock().unwrap() = Some(DEFAULT_HOTKEY.to_string());
    }
    Ok(result)
}

/// Tauri command: saves `combo` as the new global hotkey, unregistering the
/// previous one first. Returns `conflict: true` when `combo` is already
/// registered elsewhere rather than erroring. Registered into the invoke
/// handler by A14 (`lib.rs`), which manages `HotkeyState`; C2/C6 later reuse
/// this same command for rebinding.
#[tauri::command]
pub fn hotkey_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, HotkeyState>,
    combo: String,
) -> Result<HotkeySetResult, String> {
    let backend = AppShortcutBackend(app);
    let previous = state.current.lock().unwrap().clone();
    let result = apply_combo(&backend, previous.as_deref(), &combo).map_err(|e| e.to_string())?;
    if result.ok {
        *state.current.lock().unwrap() = Some(combo);
    }
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
    fn invalid_combo_returns_error() {
        let backend = InMemoryRegistry::default();
        assert_eq!(
            apply_combo(&backend, None, "NotAKey"),
            Err(HotkeyError::InvalidCombo("NotAKey".to_string()))
        );
    }
}
