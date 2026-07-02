//! The Tauri composition root (A14): declares every Phase A backend module,
//! manages their stores/state, registers every Phase A command in the
//! invoke handler, and wires the global hotkey + tray.
//!
//! This is the capstone that assembles what A1b/A4-A9/A12 built; per the
//! plan it does not re-implement those modules, only glues them together.
//! The one exception is `refine`/`restore_original`/`inject_text`/
//! `cancel_refine`: `orchestrator.rs` (A5) deliberately left these as plain
//! methods on `Orchestrator` rather than `#[tauri::command]`s, since picking
//! *which* provider/model to refine with is data-dependent (the active
//! connection, chosen here at call time) rather than fixed at construction
//! — so those thin command wrappers, and the `active_provider` lookup they
//! share, live here instead.

pub mod connections;
pub mod hotkey;
pub mod orchestrator;
pub mod permission;
pub mod prompt_builder;
pub mod settings;
pub mod tray;

use std::sync::{Arc, Mutex};

use llm_provider::OpenAiCompatProvider;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

use connections::{connection_add, connection_list, ConnectionStore};
use hotkey::{hotkey_set, HotkeyState};
use orchestrator::{Orchestrator, RefineOutcome, SystemTextIo, TextCapture, TextInjector};
use permission::{permission_open_settings, permission_status, AccessibilityChecker};
use prompt_builder::BuildOptions;
use settings::{settings_get, settings_set, SettingsStore};
use tray::{tray_quit, tray_refine};

/// Rejection string `refine` returns when no connection with an enabled
/// model has been added yet. The frontend (`Capture.tsx`) matches this
/// exact string to show the "no active model" state (S19/A11).
pub const NO_ACTIVE_MODEL_ERROR: &str = "no_active_model";
/// Rejection string `refine` returns when the Accessibility permission has
/// been revoked since the app started. The frontend matches this exact
/// string to show the "permission needed" state (S36/A13).
pub const PERMISSION_DENIED_ERROR: &str = "permission_denied";

/// Settings key for the editable default refine direction (Behavior/A8).
const DEFAULT_DIRECTION_KEY: &str = "refine.default_direction";

/// Holds what a single in-flight/most-recent refine needs across separate
/// command invocations: the captured original (for `restore_original`) and
/// a cancellation token (for `cancel_refine`). Managed as Tauri state.
#[derive(Default)]
pub struct RefineState {
    restore_buffer: Mutex<Option<String>>,
    cancel_token: Mutex<CancellationToken>,
}

/// Picks the connection/model to refine with: the first stored connection
/// that has at least one enabled model. Phase A has no active-model
/// switcher yet (that's Phase B/C's Models screen + tray); this is the
/// whole app's default provider until then.
///
/// Phase A's `ConnectionStore` doesn't persist API keys (Phase B's B7b adds
/// a `key_ref`), so the provider is built with an empty key — enough for
/// keyless local endpoints (Ollama) end-to-end; cloud providers need B7b's
/// key storage before a real call succeeds.
fn active_provider(
    connections: &ConnectionStore,
) -> Result<(Arc<dyn llm_provider::LlmProvider>, String), String> {
    let stored = connections.list().map_err(|e| e.to_string())?;
    let connection = stored
        .into_iter()
        .find(|c| !c.enabled_models.is_empty())
        .ok_or_else(|| NO_ACTIVE_MODEL_ERROR.to_string())?;
    let model = connection.enabled_models[0].clone();
    let provider = OpenAiCompatProvider::new(&connection.base_url, "");
    Ok((Arc::new(provider), model))
}

/// Rejects with [`PERMISSION_DENIED_ERROR`] unless `checker` reports the
/// Accessibility permission granted.
///
/// Generic over the checker (mirrors `permission::status_from`) so this gate
/// — the exact logic `run_refine` applies — is unit-testable with a fake on
/// any platform: the real `SystemAccessibilityChecker` is always "granted"
/// off macOS (`permission.rs`), so the denied branch is otherwise
/// unreachable in CI.
fn permission_gate<C: AccessibilityChecker>(checker: &C) -> Result<(), String> {
    if !permission::status_from(checker).granted {
        return Err(PERMISSION_DENIED_ERROR.to_string());
    }
    Ok(())
}

/// Runs the refine pipeline against an already-selected `model`/`provider`
/// and a given capture/inject seam, recording the result in `refine_state`.
///
/// Generic over `TextCapture`/`TextInjector` (mirrors `Orchestrator`'s own
/// genericity) and takes the provider directly rather than looking it up
/// itself, so the whole post-permission-check pipeline — settings read,
/// orchestrator run, restore-buffer bookkeeping — is unit-testable with
/// fakes (`orchestrator`'s `FakeProvider`/`FakeCapture`/`FakeInjector`)
/// instead of a live network call.
async fn execute_refine<C: TextCapture, I: TextInjector>(
    refine_state: &RefineState,
    settings: &SettingsStore,
    model: String,
    provider: Arc<dyn llm_provider::LlmProvider>,
    capture: C,
    injector: I,
) -> Result<RefineOutcome, String> {
    let direction = settings
        .get(DEFAULT_DIRECTION_KEY)
        .map_err(|e| e.to_string())?;
    let opts = BuildOptions {
        direction,
        model,
        ..BuildOptions::default()
    };

    let cancel = CancellationToken::new();
    *refine_state.cancel_token.lock().unwrap() = cancel.clone();

    let orch = Orchestrator::new(capture, injector, provider);
    let outcome = orch
        .refine(&opts, cancel)
        .await
        .map_err(|e| e.to_string())?;
    *refine_state.restore_buffer.lock().unwrap() = Some(outcome.original.clone());
    Ok(outcome)
}

/// The shared implementation behind both the `refine` command and the
/// tray's Refine entry (`tray::tray_refine`) and (eventually) the global
/// hotkey handler — one pipeline, three triggers.
pub(crate) async fn run_refine<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RefineOutcome, String> {
    permission_gate(&permission::SystemAccessibilityChecker)?;

    let connections = app.state::<ConnectionStore>();
    let (provider, model) = active_provider(&connections)?;

    let settings = app.state::<SettingsStore>();
    let refine_state = app.state::<RefineState>();
    execute_refine(
        &refine_state,
        &settings,
        model,
        provider,
        SystemTextIo,
        SystemTextIo,
    )
    .await
}

/// Tauri command: runs the default refine pipeline (capture -> prompt ->
/// model -> blind inject). Rejects with [`NO_ACTIVE_MODEL_ERROR`] or
/// [`PERMISSION_DENIED_ERROR`] for those specific failure modes; any other
/// failure rejects with a generic message and injects nothing.
#[tauri::command]
async fn refine<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<RefineOutcome, String> {
    run_refine(&app).await
}

/// Returns the text saved by the most recent `refine` call, or an error if
/// nothing has been captured yet. The plain logic behind the
/// `restore_original` command, taking `&RefineState` directly rather than a
/// `tauri::State` so it's unit-testable without a running app.
fn restore_original_buffer(refine_state: &RefineState) -> Result<String, String> {
    refine_state
        .restore_buffer
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no captured original to restore".to_string())
}

/// Tauri command: returns the original text saved by the most recent
/// `refine` call, for restore. Does not itself inject anything — the
/// frontend pairs this with `inject_text` (mirrors `capture-restore`'s
/// declared backend commands in `controls/capture.json`).
#[tauri::command]
fn restore_original(refine_state: tauri::State<'_, RefineState>) -> Result<String, String> {
    restore_original_buffer(&refine_state)
}

/// Tauri command: injects `text` into the focused app in place of the
/// current selection.
#[tauri::command]
fn inject_text(text: String) -> Result<(), String> {
    text_inject::inject(&text).map_err(|e| e.to_string())
}

/// Cancels the in-flight `refine` call, if any. The plain logic behind the
/// `cancel_refine` command, taking `&RefineState` directly (see
/// `restore_original_buffer`).
fn cancel_refine_token(refine_state: &RefineState) {
    refine_state.cancel_token.lock().unwrap().cancel();
}

/// Tauri command: cancels the in-flight `refine` call, if any.
#[tauri::command]
fn cancel_refine(refine_state: tauri::State<'_, RefineState>) {
    cancel_refine_token(&refine_state);
}

/// Builds the invoke handler used by the production app. Exposed so
/// `tests/wireup_test.rs` can assert the exact registered command set
/// (including `tray_refine`/`tray_quit`) without spinning up a real
/// window/OS integration.
pub fn invoke_handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool {
    tauri::generate_handler![
        settings_get,
        settings_set,
        permission_status,
        permission_open_settings,
        hotkey_set,
        connection_add,
        connection_list,
        refine,
        restore_original,
        inject_text,
        cancel_refine,
        tray_refine,
        tray_quit,
    ]
}

/// Opens (creating on first run) the settings/connections SQLite stores
/// under the app's data directory.
fn open_stores<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> anyhow::Result<(SettingsStore, ConnectionStore)> {
    let app_data = app.path().app_data_dir()?;
    std::fs::create_dir_all(&app_data)?;
    let settings = SettingsStore::open(&app_data.join("settings.sqlite3"))?;
    let connections = ConnectionStore::open(&app_data.join("connections.sqlite3"))?;
    Ok((settings, connections))
}

/// The Tauri app's entry point, called by `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            let (settings, connections) = open_stores(&handle)
                .map_err(|e| format!("failed to open settings/connections stores: {e}"))?;
            app.manage(settings);
            app.manage(connections);
            app.manage(HotkeyState::default());
            app.manage(RefineState::default());

            // Global hotkey: best-effort. A headless/CI environment (or one
            // without OS-level shortcut support) shouldn't prevent the rest
            // of the app from starting.
            let hotkey_state = app.state::<HotkeyState>();
            if let Err(e) = hotkey::register_default(&handle, &hotkey_state) {
                eprintln!("[hotkey] failed to register default hotkey: {e}");
            }

            if let Err(e) = tray::setup_tray(&handle) {
                eprintln!("[tray] failed to set up tray: {e}");
            }

            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result as AnyResult;
    use async_trait::async_trait;
    use llm_provider::{LlmProvider, LlmRequest, LlmResponse};

    // ---- active_provider ----

    fn new_connections() -> ConnectionStore {
        ConnectionStore::open_in_memory().expect("failed to open in-memory connections")
    }

    #[test]
    fn active_provider_errs_when_no_connection_has_an_enabled_model() {
        let connections = new_connections();
        assert_eq!(
            active_provider(&connections).err(),
            Some(NO_ACTIVE_MODEL_ERROR.to_string())
        );
    }

    #[test]
    fn active_provider_returns_the_first_connection_with_an_enabled_model() {
        let connections = new_connections();
        // No enabled models yet -> skipped by `active_provider`.
        connections.add("openai", "https://api.openai.com", &[]).unwrap();
        connections
            .add("ollama", "http://localhost:11434", &["llama3".to_string()])
            .unwrap();

        let (_, model) = active_provider(&connections).expect("should find an enabled model");
        assert_eq!(model, "llama3");
    }

    // ---- permission_gate ----

    struct FakeChecker(bool);

    impl AccessibilityChecker for FakeChecker {
        fn is_trusted(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn permission_gate_rejects_when_not_granted() {
        assert_eq!(
            permission_gate(&FakeChecker(false)),
            Err(PERMISSION_DENIED_ERROR.to_string())
        );
    }

    #[test]
    fn permission_gate_allows_when_granted() {
        assert_eq!(permission_gate(&FakeChecker(true)), Ok(()));
    }

    // ---- restore_original_buffer / cancel_refine_token ----

    #[test]
    fn restore_original_buffer_errs_when_nothing_has_been_captured() {
        let state = RefineState::default();
        assert!(restore_original_buffer(&state).is_err());
    }

    #[test]
    fn restore_original_buffer_returns_the_populated_buffer() {
        let state = RefineState::default();
        *state.restore_buffer.lock().unwrap() = Some("original text".to_string());
        assert_eq!(
            restore_original_buffer(&state),
            Ok("original text".to_string())
        );
    }

    #[test]
    fn cancel_refine_token_cancels_the_current_token() {
        let state = RefineState::default();
        let token = state.cancel_token.lock().unwrap().clone();
        assert!(!token.is_cancelled());

        cancel_refine_token(&state);

        assert!(token.is_cancelled());
    }

    // ---- inject_text ----

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn inject_text_reports_the_platform_unsupported_error() {
        // `text-inject` only implements real injection on macOS; off macOS
        // this exercises `inject_text`'s error-mapping without touching any
        // real OS API.
        assert!(inject_text("hello".to_string()).is_err());
    }

    // ---- execute_refine (happy path, with fakes) ----

    struct FakeCapture(String);

    impl TextCapture for FakeCapture {
        fn capture(&self) -> AnyResult<String> {
            Ok(self.0.clone())
        }
    }

    #[derive(Clone, Default)]
    struct FakeInjector {
        injected: Arc<Mutex<Vec<String>>>,
    }

    impl FakeInjector {
        fn injected(&self) -> Vec<String> {
            self.injected.lock().unwrap().clone()
        }
    }

    impl TextInjector for FakeInjector {
        fn inject(&self, text: &str) -> AnyResult<()> {
            self.injected.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    struct FakeProvider(String);

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> AnyResult<LlmResponse> {
            Ok(LlmResponse {
                text: self.0.clone(),
                model: "fake-model".to_string(),
                usage_tokens: None,
            })
        }

        async fn list_models(&self) -> AnyResult<Vec<String>> {
            Ok(vec!["fake-model".to_string()])
        }

        async fn is_available(&self) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }
    }

    #[tokio::test]
    async fn execute_refine_runs_the_pipeline_and_fills_the_restore_buffer() {
        let refine_state = RefineState::default();
        let settings = SettingsStore::open_in_memory().expect("failed to open in-memory settings");
        let injector = FakeInjector::default();

        let outcome = execute_refine(
            &refine_state,
            &settings,
            "fake-model".to_string(),
            Arc::new(FakeProvider("refined text".to_string())),
            FakeCapture("original text".to_string()),
            injector.clone(),
        )
        .await
        .expect("execute_refine should succeed");

        assert_eq!(outcome.original, "original text");
        assert_eq!(outcome.refined, "refined text");
        assert_eq!(injector.injected(), vec!["refined text".to_string()]);
        // The restore buffer (read by `restore_original`) must be filled too.
        assert_eq!(
            restore_original_buffer(&refine_state),
            Ok("original text".to_string())
        );
    }
}
