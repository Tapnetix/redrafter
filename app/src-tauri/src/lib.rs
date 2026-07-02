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
use orchestrator::{Orchestrator, RefineOutcome, SystemTextIo};
use permission::{permission_open_settings, permission_status};
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

/// The shared implementation behind both the `refine` command and the
/// tray's Refine entry (`tray::tray_refine`) and (eventually) the global
/// hotkey handler — one pipeline, three triggers.
pub(crate) async fn run_refine<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RefineOutcome, String> {
    if !permission::status_from(&permission::SystemAccessibilityChecker).granted {
        return Err(PERMISSION_DENIED_ERROR.to_string());
    }

    let connections = app.state::<ConnectionStore>();
    let (provider, model) = active_provider(&connections)?;

    let settings = app.state::<SettingsStore>();
    let direction = settings
        .get(DEFAULT_DIRECTION_KEY)
        .map_err(|e| e.to_string())?;
    let opts = BuildOptions {
        direction,
        model,
        ..BuildOptions::default()
    };

    let refine_state = app.state::<RefineState>();
    let cancel = CancellationToken::new();
    *refine_state.cancel_token.lock().unwrap() = cancel.clone();

    let orch = Orchestrator::new(SystemTextIo, SystemTextIo, provider);
    let outcome = orch
        .refine(&opts, cancel)
        .await
        .map_err(|e| e.to_string())?;
    *refine_state.restore_buffer.lock().unwrap() = Some(outcome.original.clone());
    Ok(outcome)
}

/// Tauri command: runs the default refine pipeline (capture -> prompt ->
/// model -> blind inject). Rejects with [`NO_ACTIVE_MODEL_ERROR`] or
/// [`PERMISSION_DENIED_ERROR`] for those specific failure modes; any other
/// failure rejects with a generic message and injects nothing.
#[tauri::command]
async fn refine<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<RefineOutcome, String> {
    run_refine(&app).await
}

/// Tauri command: returns the original text saved by the most recent
/// `refine` call, for restore. Does not itself inject anything — the
/// frontend pairs this with `inject_text` (mirrors `capture-restore`'s
/// declared backend commands in `controls/capture.json`).
#[tauri::command]
fn restore_original(refine_state: tauri::State<'_, RefineState>) -> Result<String, String> {
    refine_state
        .restore_buffer
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no captured original to restore".to_string())
}

/// Tauri command: injects `text` into the focused app in place of the
/// current selection.
#[tauri::command]
fn inject_text(text: String) -> Result<(), String> {
    text_inject::inject(&text).map_err(|e| e.to_string())
}

/// Tauri command: cancels the in-flight `refine` call, if any.
#[tauri::command]
fn cancel_refine(refine_state: tauri::State<'_, RefineState>) {
    refine_state.cancel_token.lock().unwrap().cancel();
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
