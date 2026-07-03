//! The Tauri composition root (A14, extended B23): declares every backend
//! module, manages their stores/state, registers every command in the
//! invoke handler, and wires the global hotkey + tray.
//!
//! This is the capstone that assembles what A1b/A4-A9/A12 (Phase A) and
//! B7/B7b/B8/B9/B10/B17 (Phase B) built; per the plan it does not
//! re-implement those modules, only glues them together. The one exception
//! is `refine`/`restore_original`/`inject_text`/`cancel_refine`:
//! `orchestrator.rs` (A5) deliberately left these as plain methods on
//! `Orchestrator` rather than `#[tauri::command]`s, since picking *which*
//! provider/model to refine with is data-dependent (the active connection,
//! chosen here at call time) rather than fixed at construction — so those
//! thin command wrappers, and the `active_provider` lookup they share, live
//! here instead.
//!
//! B23 also reconciles three seams Phase B's screen tasks deliberately left
//! open (each documented at its own site below/in the module it touches):
//!   - **Review-mode pending review** (B5/B6): `inject_text`/`cancel_refine`
//!     now also resolve/clear `RefineState::pending_review`, so a review-mode
//!     refine's accept/edit/discard actually clears the pending state
//!     instead of leaving it stale (see `clear_pending_review`).
//!   - **Real provider keys** (B7b/B10): `active_provider`/
//!     `connection_refresh_models` resolve a connection's API key through
//!     `secrets::secrets_get` first, falling back to the plaintext DB column
//!     (see `connections::resolve_api_key`), so a real cloud connection's
//!     refine actually authenticates.
//!   - **Paused gate** (B17): `run_refine` -- the one pipeline `refine`,
//!     `tray_refine`, and the global hotkey (`dispatch_hotkey`) all funnel
//!     through -- now no-ops with [`PAUSED_ERROR`] while `tray_pause` has
//!     paused capturing.

pub mod command_parser;
pub mod connections;
pub mod hotkey;
pub mod models;
pub mod orchestrator;
pub mod permission;
pub mod prompt_builder;
pub mod quote_parser;
pub mod secrets;
pub mod settings;
pub mod tray;

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use tauri::Manager;
use tauri_plugin_global_shortcut::{Shortcut, ShortcutEvent, ShortcutState};
use tokio_util::sync::CancellationToken;

use connections::{
    connection_add, connection_edit, connection_list, connection_refresh_models,
    connection_remove, connection_test, model_add_manual, resolve_api_key, ConnectionStore,
};
use hotkey::{hotkey_set, HotkeyState};
use models::{
    model_disable, model_set_active, model_toggle_favorite, models_list, ollama_pull,
};
use orchestrator::{
    FallbackTarget, InjectMode, Orchestrator, RefineFlow, RefineOutcome, SystemTextIo, TextCapture,
    TextInjector,
};
use permission::{permission_open_settings, permission_status, AccessibilityChecker};
use prompt_builder::{BuildOptions, QuoteMode};
use secrets::{secrets_delete, secrets_set, secrets_set_key, SecretStore};
use settings::{settings_get, settings_set, SettingsStore};
use tray::{
    tray_check_updates, tray_pause, tray_quit, tray_refine, tray_resume, tray_set_active_model,
    tray_set_launch_login,
};

/// Rejection string `refine` returns when no connection with an enabled
/// model has been added yet. The frontend (`Capture.tsx`) matches this
/// exact string to show the "no active model" state (S19/A11).
pub const NO_ACTIVE_MODEL_ERROR: &str = "no_active_model";
/// Rejection string `refine` returns when the Accessibility permission has
/// been revoked since the app started. The frontend matches this exact
/// string to show the "permission needed" state (S36/A13).
pub const PERMISSION_DENIED_ERROR: &str = "permission_denied";
/// Rejection string `refine`/`tray_refine`/the global hotkey return while
/// capturing is paused (B17's `tray_pause`, reconciled by B23 -- see the
/// module docs' "Paused gate" note).
pub const PAUSED_ERROR: &str = "paused";

/// Settings key for the editable default refine direction (Behavior/A8).
const DEFAULT_DIRECTION_KEY: &str = "refine.default_direction";
/// Settings key for the configured inject mode (`"blind"`/`"review"`,
/// Behavior/B6). Unset or any other value defaults to
/// [`InjectMode::Blind`] -- Phase A's only behavior.
const INJECT_MODE_KEY: &str = "behavior.inject_mode";
/// Settings key for the quote-handling mode (`"answer"`/`"answer_quote"`/
/// `"rd"`, Behavior/B6b's `behavior.quote_mode`). Unset or any other value
/// defaults to [`QuoteMode`]'s default (`IncludeQuote`), matching the
/// Behavior screen's own default selection.
const QUOTE_MODE_KEY: &str = "behavior.quote_mode";
/// Settings key for the on-failure strategy (`"notify"`/`"fallback"`,
/// Behavior/B6b's `behavior.on_failure`). Only `"fallback"` engages the
/// configured [`FALLBACK_CHAIN_KEY`]; anything else runs with no fallbacks.
const ON_FAILURE_KEY: &str = "behavior.on_failure";
/// Settings key for the ordered fallback chain (Behavior/B6b's
/// `behavior.fallback_chain`): a JSON array of model-id strings (e.g.
/// `["gpt-5.1", "qwen3:8b"]`), each resolved to a connection that has it
/// enabled. Only consulted when [`ON_FAILURE_KEY`] is `"fallback"`.
const FALLBACK_CHAIN_KEY: &str = "behavior.fallback_chain";
/// Settings key `tray_pause`/`tray_resume` (B17/B23) persist the paused flag
/// under; also read by [`run_refine`]'s paused gate. Shared with the
/// frontend's own `Tray.tsx`, which persists/reads the same key name.
pub(crate) const PAUSED_SETTING_KEY: &str = "paused";

/// Holds what a single in-flight/most-recent refine needs across separate
/// command invocations: the captured original (for `restore_original`), a
/// cancellation token (for `cancel_refine`), and (B23) the most recent
/// review-mode result still awaiting the user's accept/edit/discard
/// decision (for `inject_text`/`cancel_refine` to resolve/clear -- see the
/// module docs' "Review-mode pending review" note). Managed as Tauri state.
#[derive(Default)]
pub struct RefineState {
    restore_buffer: Mutex<Option<String>>,
    cancel_token: Mutex<CancellationToken>,
    pending_review: Mutex<Option<RefineOutcome>>,
}

/// Picks the connection/model to refine with, honoring the user's active-
/// model choice (Models screen/B8, tray/B9 — persisted as an
/// `ActiveModelRef` under the `active_model` setting via
/// `models::model_set_active`/`tray::tray_set_active_model`):
///
///   - **Active model set**: resolve its connection + model. If that model
///     is still enabled on that connection, build the vendor-native provider
///     for it. If the active ref points at a connection/model that no longer
///     exists or has been disabled/removed, reject with
///     [`NO_ACTIVE_MODEL_ERROR`] (routing the user to pick another) rather
///     than silently substituting a different model — this is the real
///     backend half of the Models screen's "active model unavailable" state
///     (S26/B21).
///   - **Active model unset** (the default before the user has ever chosen
///     one): fall back to the first stored connection that has at least one
///     enabled model, using its first enabled model.
///
/// Either way, resolves the connection's API key via
/// `connections::resolve_api_key` (secure storage first, the plaintext DB
/// column as fallback -- B23's reconciliation of B7b/B10) and builds the
/// vendor-appropriate provider via `connections::provider_for`, rather than
/// always assuming an OpenAI-compatible endpoint with no key.
fn active_provider(
    connections: &ConnectionStore,
    settings: &SettingsStore,
    secrets: &SecretStore,
) -> Result<(Arc<dyn llm_provider::LlmProvider>, String), String> {
    match models::active_model_ref(settings).map_err(|e| e.to_string())? {
        Some((connection_id, model_id)) => {
            // The user picked a specific model: use exactly that one, or
            // reject if it's no longer enabled/available (disabled/removed).
            let connection = connections
                .get(&connection_id)
                .map_err(|e| e.to_string())?
                .filter(|c| c.enabled_models.contains(&model_id))
                .ok_or_else(|| NO_ACTIVE_MODEL_ERROR.to_string())?;
            build_provider(connections, secrets, &connection, model_id)
        }
        None => {
            // No active model chosen yet: default to the first connection
            // with an enabled model (Phase B's pre-active-model behavior).
            let stored = connections.list().map_err(|e| e.to_string())?;
            let connection = stored
                .into_iter()
                .find(|c| !c.enabled_models.is_empty())
                .ok_or_else(|| NO_ACTIVE_MODEL_ERROR.to_string())?;
            let model = connection.enabled_models[0].clone();
            build_provider(connections, secrets, &connection, model)
        }
    }
}

/// Resolves `connection`'s API key (secure storage first, plaintext DB
/// column as fallback) and builds its vendor-native provider paired with
/// `model` — the shared tail of [`active_provider`]'s two branches and each
/// [`FallbackTarget`] built by [`resolve_fallback_targets`].
fn build_provider(
    connections: &ConnectionStore,
    secrets: &SecretStore,
    connection: &connections::Connection,
    model: String,
) -> Result<(Arc<dyn llm_provider::LlmProvider>, String), String> {
    let api_key =
        resolve_api_key(connections, secrets, &connection.id).map_err(|e| e.to_string())?;
    let provider =
        connections::provider_for(&connection.provider_kind, &connection.base_url, &api_key);
    Ok((Arc::from(provider), model))
}

/// Builds the ordered fallback chain the orchestrator should try when the
/// primary model call fails, from the Behavior screen's
/// `behavior.on_failure`/`behavior.fallback_chain` settings (B6b):
///
///   - Unless on-failure is set to `"fallback"`, returns an empty list (the
///     `"notify"` default — fail loudly, inject nothing, no fallback).
///   - Otherwise parses `behavior.fallback_chain` (a JSON array of model-id
///     strings, exactly what `Behavior.tsx` writes) and resolves each id to
///     the first stored connection that has it enabled, building that
///     connection's vendor-native provider. Ids that match no enabled
///     connection are skipped (a best-effort chain — an unresolvable entry
///     shouldn't abort the whole refine).
///
/// forward-ref: `behavior.retry_count` (also written by `Behavior.tsx`) has
/// no orchestrator retry concept to consume yet — the chain tries each model
/// once. Wiring per-model retries is Phase C scope; the setting is read
/// there once that loop exists.
fn resolve_fallback_targets(
    connections: &ConnectionStore,
    settings: &SettingsStore,
    secrets: &SecretStore,
) -> Result<Vec<FallbackTarget>, String> {
    let on_failure = settings.get(ON_FAILURE_KEY).map_err(|e| e.to_string())?;
    if on_failure.as_deref() != Some("fallback") {
        return Ok(Vec::new());
    }

    let model_ids: Vec<String> = match settings.get(FALLBACK_CHAIN_KEY).map_err(|e| e.to_string())? {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        None => Vec::new(),
    };

    let stored = connections.list().map_err(|e| e.to_string())?;
    let mut targets = Vec::new();
    for model_id in model_ids {
        if let Some(connection) = stored
            .iter()
            .find(|c| c.enabled_models.contains(&model_id))
        {
            let (provider, model) =
                build_provider(connections, secrets, connection, model_id.clone())?;
            targets.push(FallbackTarget::new(provider, model));
        }
    }
    Ok(targets)
}

/// Reads the configured quote-handling mode (Behavior/B6b's
/// `behavior.quote_mode` setting) as a [`QuoteMode`], defaulting to
/// [`QuoteMode`]'s own default when unset or unrecognized (matching the
/// Behavior screen's default selection).
fn quote_mode_from_settings(settings: &SettingsStore) -> Result<QuoteMode, String> {
    let raw = settings.get(QUOTE_MODE_KEY).map_err(|e| e.to_string())?;
    Ok(match raw.as_deref() {
        Some("answer") => QuoteMode::AnswerOnly,
        Some("answer_quote") => QuoteMode::IncludeQuote,
        Some("rd") => QuoteMode::LetDirectionDecide,
        _ => QuoteMode::default(),
    })
}

/// Reads the configured inject mode (Behavior/B6's `behavior.inject_mode`
/// setting) as an [`InjectMode`], defaulting to [`InjectMode::Blind`] when
/// unset or unrecognized.
fn inject_mode_from_settings(settings: &SettingsStore) -> Result<InjectMode, String> {
    let raw = settings.get(INJECT_MODE_KEY).map_err(|e| e.to_string())?;
    Ok(match raw.as_deref() {
        Some("review") => InjectMode::Review,
        _ => InjectMode::Blind,
    })
}

/// Whether capturing is currently paused (`tray_pause`/`tray_resume`, B17).
pub(crate) fn is_paused(settings: &SettingsStore) -> bool {
    settings
        .get(PAUSED_SETTING_KEY)
        .ok()
        .flatten()
        .as_deref()
        == Some("true")
}

/// Persists the paused flag `tray_pause`/`tray_resume` (`tray.rs`) toggle,
/// and [`is_paused`]/[`run_refine`]'s gate read back.
pub(crate) fn set_paused(settings: &SettingsStore, paused: bool) -> Result<(), String> {
    settings
        .set(PAUSED_SETTING_KEY, if paused { "true" } else { "false" })
        .map_err(|e| e.to_string())
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
/// and a given capture/inject seam, recording the result in `refine_state`
/// (including, for a review-mode result, `refine_state.pending_review` --
/// B23's reconciliation of B5's review branch, see the module docs).
///
/// Generic over `TextCapture`/`TextInjector` (mirrors `Orchestrator`'s own
/// genericity) and takes the provider directly rather than looking it up
/// itself, so the whole post-permission-check pipeline — settings read,
/// orchestrator run, restore-buffer/pending-review bookkeeping — is
/// unit-testable with fakes (`orchestrator`'s `FakeProvider`/`FakeCapture`/
/// `FakeInjector`) instead of a live network call.
async fn execute_refine<C: TextCapture, I: TextInjector>(
    refine_state: &RefineState,
    settings: &SettingsStore,
    model: String,
    provider: Arc<dyn llm_provider::LlmProvider>,
    fallbacks: Vec<FallbackTarget>,
    capture: C,
    injector: I,
) -> Result<RefineFlow, String> {
    let direction = settings
        .get(DEFAULT_DIRECTION_KEY)
        .map_err(|e| e.to_string())?;
    let mode = inject_mode_from_settings(settings)?;
    let quote_mode = quote_mode_from_settings(settings)?;
    let opts = BuildOptions {
        direction,
        model,
        quote_mode,
        ..BuildOptions::default()
    };

    let cancel = CancellationToken::new();
    *refine_state.cancel_token.lock().unwrap() = cancel.clone();

    let orch = Orchestrator::new(capture, injector, provider);
    let flow = orch
        .refine_with(&opts, &fallbacks, mode, cancel)
        .await
        .map_err(|e| e.to_string())?;

    let outcome = flow.clone().into_outcome();
    *refine_state.restore_buffer.lock().unwrap() = Some(outcome.original.clone());
    *refine_state.pending_review.lock().unwrap() = match &flow {
        RefineFlow::PendingReview(_) => Some(outcome),
        RefineFlow::Injected(_) => None,
    };
    Ok(flow)
}

/// The shared implementation behind the `refine` command, the tray's
/// Refine entry (`tray::tray_refine`), and the global hotkey
/// (`dispatch_hotkey`, below) — one pipeline, three triggers. No-ops with
/// [`PAUSED_ERROR`] while capturing is paused (`tray_pause`, B17/B23 — see
/// the module docs' "Paused gate" note), checked before anything else so a
/// paused hotkey press never touches Accessibility/the network.
///
/// Takes the stores directly (rather than an `AppHandle`) so the whole
/// gate-then-pipeline sequence — including the paused gate's precedence
/// over the permission/active-model checks — is unit-testable with
/// directly-constructed, in-memory stores instead of a built `tauri::App`
/// (a real app build initializes the tray-icon plugin, which requires the
/// main thread and SIGSEGVs/errors under a test harness that runs tests off
/// it; see `run_refine`, this function's thin `AppHandle`-unwrapping
/// wrapper).
async fn run_refine_with(
    settings: &SettingsStore,
    connections: &ConnectionStore,
    secrets: &SecretStore,
    refine_state: &RefineState,
) -> Result<RefineFlow, String> {
    if is_paused(settings) {
        return Err(PAUSED_ERROR.to_string());
    }

    permission_gate(&permission::SystemAccessibilityChecker)?;

    let (provider, model) = active_provider(connections, settings, secrets)?;
    let fallbacks = resolve_fallback_targets(connections, settings, secrets)?;

    execute_refine(
        refine_state,
        settings,
        model,
        provider,
        fallbacks,
        SystemTextIo,
        SystemTextIo,
    )
    .await
}

/// Thin `AppHandle` wrapper around [`run_refine_with`], pulling the four
/// managed stores out of Tauri state. This is the one piece that genuinely
/// needs a running app/handle; the actual gate-and-pipeline logic it
/// delegates to is unit-tested directly (see `run_refine_with`'s tests).
pub(crate) async fn run_refine<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RefineFlow, String> {
    run_refine_with(
        &app.state::<SettingsStore>(),
        &app.state::<ConnectionStore>(),
        &app.state::<SecretStore>(),
        &app.state::<RefineState>(),
    )
    .await
}

/// Tauri command: runs the default refine pipeline (capture -> prompt ->
/// model -> inject, or -- when the configured inject mode is `Review` --
/// suspend for the user's accept/edit/discard, see [`RefineFlow`]). Rejects
/// with [`NO_ACTIVE_MODEL_ERROR`], [`PERMISSION_DENIED_ERROR`], or
/// [`PAUSED_ERROR`] for those specific failure modes; any other failure
/// rejects with a generic message and injects nothing.
#[tauri::command]
async fn refine<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<RefineFlow, String> {
    run_refine(&app).await
}

/// Reacts to an OS-level event for any hotkey registered through
/// `tauri-plugin-global-shortcut` — the plugin dispatches every registered
/// shortcut's events through the one `.with_handler` closure `run` installs
/// below, not just whichever combo is "current" (`register_default` at
/// startup and `hotkey_set` rebinds both go through the same plugin
/// manager). This is that closure's body, pulled out so it's unit-testable
/// under `tauri::test::MockRuntime` with a synthetic `Shortcut`/
/// `ShortcutEvent` instead of a real OS keypress.
///
/// Fires the shared `run_refine` pipeline exactly when this is a key-down
/// (`Pressed`) event for whichever combo `HotkeyState` currently considers
/// active — a stale/replaced combo (e.g. one left registered momentarily
/// during a `hotkey_set` rebind) is ignored rather than double-triggering.
/// Mirrors the tray's "Refine" entry (`tray::handle_menu_event`): both
/// funnel into the same pipeline rather than duplicating it.
///
/// Returns the spawned task's `JoinHandle` (the real `.with_handler`
/// closure drops it — detaching, not aborting, the task) so tests can await
/// it and assert the pipeline actually ran (by its result), rather than
/// merely "didn't panic".
pub(crate) fn dispatch_hotkey<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    hotkey_state: &HotkeyState,
    shortcut: &Shortcut,
    event: ShortcutEvent,
) -> Option<tauri::async_runtime::JoinHandle<Result<RefineFlow, String>>> {
    if !should_dispatch_hotkey(hotkey_state, shortcut, &event) {
        return None;
    }

    let handle = app.clone();
    Some(tauri::async_runtime::spawn(async move {
        run_refine(&handle).await
    }))
}

/// The routing decision `dispatch_hotkey` applies: fires exactly for a
/// key-down (`Pressed`) event whose `shortcut` matches whichever combo
/// `hotkey_state` currently considers active (a stale/replaced combo, e.g.
/// one left registered momentarily during a `hotkey_set` rebind, is
/// ignored). Pulled out as a plain, app-free function so this decision is
/// unit-testable without a `tauri::App`/`AppHandle` at all (mirrors
/// `run_refine_with`'s extraction for the same "no mock app" reason).
fn should_dispatch_hotkey(
    hotkey_state: &HotkeyState,
    shortcut: &Shortcut,
    event: &ShortcutEvent,
) -> bool {
    if event.state != ShortcutState::Pressed {
        return false;
    }

    hotkey_state
        .current
        .lock()
        .unwrap()
        .as_deref()
        .and_then(|combo| Shortcut::from_str(combo).ok())
        .is_some_and(|current| current == *shortcut)
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

/// Clears any pending review result. Shared by [`inject_text`] (an
/// injection -- whether a review accept/edit-accept or a plain restore --
/// supersedes a stale pending draft) and [`cancel_refine`] (an explicit
/// discard). Plain logic over `&RefineState`, mirroring
/// `restore_original_buffer`/`cancel_refine_token`, so it's unit-testable
/// without a real OS inject call.
fn clear_pending_review(refine_state: &RefineState) {
    *refine_state.pending_review.lock().unwrap() = None;
}

/// Injects `text`, then clears any pending review result (see
/// `clear_pending_review`). The plain logic behind the `inject_text`
/// command, taking `&RefineState` directly so it's unit-testable without a
/// `tauri::State` harness (mirrors `restore_original_buffer`).
fn inject_text_impl(refine_state: &RefineState, text: &str) -> Result<(), String> {
    text_inject::inject(text).map_err(|e| e.to_string())?;
    clear_pending_review(refine_state);
    Ok(())
}

/// Tauri command: injects `text` into the focused app in place of the
/// current selection. Used both for a plain restore and for a review-mode
/// accept/edit-accept (`Capture.tsx`) -- either way, a pending review result
/// is now resolved (see `clear_pending_review`).
#[tauri::command]
fn inject_text(refine_state: tauri::State<'_, RefineState>, text: String) -> Result<(), String> {
    inject_text_impl(&refine_state, &text)
}

/// Cancels the in-flight `refine` call, if any, and discards any pending
/// review result (see `clear_pending_review`) -- the plain logic behind the
/// `cancel_refine` command, taking `&RefineState` directly (see
/// `restore_original_buffer`).
fn cancel_refine_token(refine_state: &RefineState) {
    refine_state.cancel_token.lock().unwrap().cancel();
    clear_pending_review(refine_state);
}

/// Tauri command: cancels the in-flight `refine` call, if any, and discards
/// any pending review result (`Capture.tsx`'s Discard action, B5/B6, now
/// resolved by B23 — see the module docs' "Review-mode pending review"
/// note).
#[tauri::command]
fn cancel_refine(refine_state: tauri::State<'_, RefineState>) {
    cancel_refine_token(&refine_state);
}

/// Builds the invoke handler used by the production app. Exposed so
/// `tests/wireup_test.rs` can assert the exact registered command set
/// (Phase A's plus every Phase B command B23 wires up) without spinning up a
/// real window/OS integration.
pub fn invoke_handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool {
    tauri::generate_handler![
        settings_get,
        settings_set,
        permission_status,
        permission_open_settings,
        hotkey_set,
        connection_add,
        connection_list,
        connection_edit,
        connection_remove,
        connection_test,
        connection_refresh_models,
        model_add_manual,
        models_list,
        model_set_active,
        model_disable,
        model_toggle_favorite,
        ollama_pull,
        secrets_set,
        secrets_set_key,
        secrets_delete,
        refine,
        restore_original,
        inject_text,
        cancel_refine,
        tray_refine,
        tray_quit,
        tray_set_active_model,
        tray_pause,
        tray_resume,
        tray_check_updates,
        tray_set_launch_login,
    ]
}

/// Opens (creating on first run) the settings/connections/secrets stores
/// under the app's data directory.
fn open_stores<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> anyhow::Result<(SettingsStore, ConnectionStore, SecretStore)> {
    let app_data = app.path().app_data_dir()?;
    std::fs::create_dir_all(&app_data)?;
    let settings = SettingsStore::open(&app_data.join("settings.sqlite3"))?;
    let connections = ConnectionStore::open(&app_data.join("connections.sqlite3"))?;
    let secrets = SecretStore::open(&app_data)?;
    // Honor a previously persisted storage-backend choice (`secrets_set`,
    // B10/B23) immediately -- otherwise every restart would silently reset
    // to the encrypted-file default until the user re-toggled the picker.
    secrets.set_storage_backend(secrets::storage_backend_from_settings(&settings));
    Ok((settings, connections, secrets))
}

/// The Tauri app's entry point, called by `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let hotkey_state = app.state::<HotkeyState>();
                    dispatch_hotkey(app, &hotkey_state, shortcut, event);
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            let (settings, connections, secrets) = open_stores(&handle)
                .map_err(|e| format!("failed to open settings/connections/secrets stores: {e}"))?;
            app.manage(settings);
            app.manage(connections);
            app.manage(secrets);
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

    fn new_settings() -> SettingsStore {
        SettingsStore::open_in_memory().expect("failed to open in-memory settings")
    }

    /// Minimal RAII temp-dir guard for a `SecretStore` in tests (mirrors
    /// `secrets.rs`'s/`connections.rs`'s own `TempDir` helpers -- `SecretStore`
    /// has no `open_in_memory`, only a file-backed `open`).
    struct TempSecretsDir(std::path::PathBuf);

    impl TempSecretsDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "redrafter_lib_secrets_{label}_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            Self(path)
        }
    }

    impl Drop for TempSecretsDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_secrets(label: &str) -> (SecretStore, TempSecretsDir) {
        let dir = TempSecretsDir::new(label);
        let store = SecretStore::open(&dir.0).expect("failed to open secret store");
        (store, dir)
    }

    #[test]
    fn active_provider_errs_when_no_connection_has_an_enabled_model() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("no-enabled-model");
        assert_eq!(
            active_provider(&connections, &settings, &secrets).err(),
            Some(NO_ACTIVE_MODEL_ERROR.to_string())
        );
    }

    #[test]
    fn active_provider_defaults_to_the_first_enabled_model_when_none_is_active() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("first-enabled-model");
        // No enabled models yet -> skipped by `active_provider`.
        connections
            .add("openai", "https://api.openai.com", None, &[])
            .unwrap();
        connections
            .add(
                "ollama",
                "http://localhost:11434",
                None,
                &["llama3".to_string()],
            )
            .unwrap();

        // With no persisted `active_model`, `active_provider` falls back to
        // the first connection that has an enabled model.
        let (_, model) = active_provider(&connections, &settings, &secrets)
            .expect("should find an enabled model");
        assert_eq!(model, "llama3");
    }

    #[test]
    fn active_provider_uses_exactly_the_persisted_active_model() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("active-model-set");
        // Two connections, each with an enabled model. The *first* is what
        // the old (buggy) "first enabled model" logic would have picked.
        connections
            .add(
                "openai",
                "https://api.openai.com",
                Some("sk"),
                &["gpt-5.1".to_string()],
            )
            .unwrap();
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                Some("sk-ant"),
                &["claude-opus-4-6".to_string()],
            )
            .unwrap();
        // The user picks the *second* connection's model as active.
        models::model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6")
            .unwrap();

        let (provider, model) = active_provider(&connections, &settings, &secrets)
            .expect("should resolve the persisted active model");

        assert_eq!(model, "claude-opus-4-6");
        // ...and its connection's vendor-native provider, not the first
        // connection's OpenAI-compatible one.
        assert_eq!(
            provider.provider_name(),
            llm_provider::AnthropicProvider::new("x", "y").provider_name()
        );
    }

    #[test]
    fn active_provider_errs_when_the_active_model_is_no_longer_enabled() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("active-model-disabled");
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                Some("sk-ant"),
                &["claude-opus-4-6".to_string()],
            )
            .unwrap();
        // Pick it active, then disable it (leaving the active ref stale --
        // exactly the S26 "active model unavailable" case).
        models::model_set_active_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6")
            .unwrap();
        models::model_disable_impl(&connections, &settings, &anthropic.id, "claude-opus-4-6")
            .unwrap();

        // It must NOT silently substitute another enabled model -- it must
        // route the user to pick one via `NO_ACTIVE_MODEL_ERROR`.
        assert_eq!(
            active_provider(&connections, &settings, &secrets).err(),
            Some(NO_ACTIVE_MODEL_ERROR.to_string())
        );
    }

    /// Boundary test for the B7b/B10 reconciliation: a connection whose key
    /// only lives in secure storage (never persisted to the plaintext DB
    /// column) still resolves through `active_provider`, proving the real
    /// call site (not just `connections::resolve_api_key` in isolation)
    /// prefers secure storage.
    #[test]
    fn active_provider_resolves_the_key_through_secure_storage() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("active-provider-secure-key");
        let anthropic = connections
            .add(
                "anthropic",
                "https://api.anthropic.com",
                None,
                &["claude-opus-4-6".to_string()],
            )
            .unwrap();
        secrets.set(&anthropic.id, "sk-ant-secure").unwrap();

        let (provider, model) = active_provider(&connections, &settings, &secrets)
            .expect("should find the anthropic connection");

        assert_eq!(model, "claude-opus-4-6");
        // `provider_for` picks the vendor-native implementation, not always
        // OpenAI-compatible -- `provider_name()` is the only externally
        // observable way to tell without a live call.
        assert_eq!(
            provider.provider_name(),
            llm_provider::AnthropicProvider::new("x", "y").provider_name()
        );
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

    #[test]
    fn cancel_refine_token_also_discards_a_pending_review_result() {
        // B23 reconciliation: `cancel_refine` (Capture.tsx's Discard, in
        // review mode) must clear `pending_review` too, not just cancel the
        // in-flight token -- see the module docs' "Review-mode pending
        // review" note.
        let state = RefineState::default();
        *state.pending_review.lock().unwrap() = Some(RefineOutcome {
            original: "orig".to_string(),
            refined: "refined".to_string(),
            model: "fake-model".to_string(),
        });

        cancel_refine_token(&state);

        assert_eq!(state.pending_review.lock().unwrap().clone(), None);
    }

    // ---- inject_text / clear_pending_review ----

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn inject_text_reports_the_platform_unsupported_error() {
        // `text-inject` only implements real injection on macOS; off macOS
        // this exercises `inject_text`'s error-mapping without touching any
        // real OS API.
        let state = RefineState::default();
        assert!(inject_text_impl(&state, "hello").is_err());
    }

    #[test]
    fn clear_pending_review_clears_a_populated_slot() {
        let state = RefineState::default();
        *state.pending_review.lock().unwrap() = Some(RefineOutcome {
            original: "orig".to_string(),
            refined: "refined".to_string(),
            model: "fake-model".to_string(),
        });

        clear_pending_review(&state);

        assert_eq!(state.pending_review.lock().unwrap().clone(), None);
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

        let flow = execute_refine(
            &refine_state,
            &settings,
            "fake-model".to_string(),
            Arc::new(FakeProvider("refined text".to_string())),
            Vec::new(),
            FakeCapture("original text".to_string()),
            injector.clone(),
        )
        .await
        .expect("execute_refine should succeed");

        assert!(matches!(flow, RefineFlow::Injected(_)), "default mode is blind");
        let outcome = flow.into_outcome();
        assert_eq!(outcome.original, "original text");
        assert_eq!(outcome.refined, "refined text");
        assert_eq!(injector.injected(), vec!["refined text".to_string()]);
        // The restore buffer (read by `restore_original`) must be filled too.
        assert_eq!(
            restore_original_buffer(&refine_state),
            Ok("original text".to_string())
        );
        // Blind mode never leaves a pending review behind.
        assert_eq!(refine_state.pending_review.lock().unwrap().clone(), None);
    }

    /// Boundary test for the B5/B23 reconciliation: a review-mode refine
    /// (`behavior.inject_mode = "review"`) suspends *without injecting* and
    /// fills `RefineState::pending_review` -- the state `inject_text`
    /// (accept) or `cancel_refine` (discard) then resolves. Cross-platform:
    /// the pipeline's own `Orchestrator` (review branch) never calls the
    /// real OS injector, so this needs no macOS.
    #[tokio::test]
    async fn review_mode_refine_leaves_a_pending_review_awaiting_the_users_choice() {
        let refine_state = RefineState::default();
        let settings = SettingsStore::open_in_memory().expect("failed to open in-memory settings");
        settings.set(INJECT_MODE_KEY, "review").unwrap();
        let injector = FakeInjector::default();

        let flow = execute_refine(
            &refine_state,
            &settings,
            "fake-model".to_string(),
            Arc::new(FakeProvider("refined text".to_string())),
            Vec::new(),
            FakeCapture("original text".to_string()),
            injector.clone(),
        )
        .await
        .expect("execute_refine should succeed");

        assert!(matches!(flow, RefineFlow::PendingReview(_)));
        assert!(injector.injected().is_empty(), "review mode must not inject yet");
        assert_eq!(
            refine_state.pending_review.lock().unwrap().clone(),
            Some(flow.into_outcome())
        );
    }

    /// The discard half of the review loop: after a review-mode refine
    /// leaves a pending result, `cancel_refine` (Capture.tsx's Discard)
    /// clears it without ever injecting. Cross-platform (discard never
    /// touches the OS injector).
    #[tokio::test]
    async fn review_mode_refine_pends_then_cancel_refine_discards_it() {
        let refine_state = RefineState::default();
        let settings = SettingsStore::open_in_memory().expect("failed to open in-memory settings");
        settings.set(INJECT_MODE_KEY, "review").unwrap();
        let injector = FakeInjector::default();

        execute_refine(
            &refine_state,
            &settings,
            "fake-model".to_string(),
            Arc::new(FakeProvider("refined text".to_string())),
            Vec::new(),
            FakeCapture("original text".to_string()),
            injector.clone(),
        )
        .await
        .expect("execute_refine should succeed");
        assert!(refine_state.pending_review.lock().unwrap().is_some());

        cancel_refine_token(&refine_state);

        assert_eq!(refine_state.pending_review.lock().unwrap().clone(), None);
        assert!(injector.injected().is_empty(), "discard must never inject");
    }

    /// The accept half of the review loop, macOS-only: `inject_text`
    /// (Capture.tsx's Accept/Edit-accept) both injects the chosen text
    /// *and* clears the pending review. Gated to macOS because `inject_text`
    /// goes through the real `text-inject` crate, which only implements
    /// injection there (off macOS it errors before reaching the clear --
    /// covered by `inject_text_reports_the_platform_unsupported_error` and
    /// the cross-platform `clear_pending_review_clears_a_populated_slot`).
    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn review_mode_refine_pends_then_inject_text_accepts_and_clears_it() {
        let refine_state = RefineState::default();
        let settings = SettingsStore::open_in_memory().expect("failed to open in-memory settings");
        settings.set(INJECT_MODE_KEY, "review").unwrap();
        let injector = FakeInjector::default();

        execute_refine(
            &refine_state,
            &settings,
            "fake-model".to_string(),
            Arc::new(FakeProvider("refined text".to_string())),
            Vec::new(),
            FakeCapture("original text".to_string()),
            injector,
        )
        .await
        .expect("execute_refine should succeed");
        assert!(refine_state.pending_review.lock().unwrap().is_some());

        inject_text_impl(&refine_state, "refined text").expect("inject should succeed on macOS");

        assert_eq!(refine_state.pending_review.lock().unwrap().clone(), None);
    }

    /// A provider whose `chat` always fails -- lets a test drive the
    /// orchestrator's fallback chain (primary fails -> a fallback target
    /// succeeds) without a live network call.
    struct FailingProvider;

    #[async_trait]
    impl LlmProvider for FailingProvider {
        async fn chat(
            &self,
            _request: &LlmRequest,
            _cancel: CancellationToken,
        ) -> AnyResult<LlmResponse> {
            anyhow::bail!("primary provider is down")
        }

        async fn list_models(&self) -> AnyResult<Vec<String>> {
            Ok(vec![])
        }

        async fn is_available(&self) -> bool {
            false
        }

        fn provider_name(&self) -> &'static str {
            "failing"
        }
    }

    /// Boundary test for Issue 2: when a fallback list is configured,
    /// `execute_refine` actually forwards it to the orchestrator -- so a
    /// primary failure is caught by a configured fallback and *that* model's
    /// output is injected (rather than the whole refine failing, as it did
    /// while the fallback slice was hard-coded empty).
    #[tokio::test]
    async fn execute_refine_forwards_the_fallback_chain_to_the_orchestrator() {
        let refine_state = RefineState::default();
        let settings = new_settings();
        let injector = FakeInjector::default();

        let fallbacks = vec![FallbackTarget::new(
            Arc::new(FakeProvider("fallback output".to_string())),
            "fallback-model".to_string(),
        )];

        let flow = execute_refine(
            &refine_state,
            &settings,
            "primary-model".to_string(),
            Arc::new(FailingProvider),
            fallbacks,
            FakeCapture("original text".to_string()),
            injector.clone(),
        )
        .await
        .expect("the fallback should rescue the failed primary");

        // The fallback's output was injected -- proving execute_refine passed
        // the non-empty fallback list through to `refine_with`.
        assert_eq!(injector.injected(), vec!["fallback output".to_string()]);
        assert_eq!(flow.into_outcome().refined, "fallback output");
    }

    /// Boundary test for Issue 2's resolution half: `resolve_fallback_targets`
    /// reads `behavior.on_failure`/`behavior.fallback_chain` and resolves the
    /// configured model ids to real, vendor-native `FallbackTarget`s (via
    /// `provider_for` + `resolve_api_key`), in the configured order.
    #[test]
    fn resolve_fallback_targets_builds_a_resolved_chain_from_the_behavior_settings() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("resolve-fallback");
        connections
            .add(
                "openai",
                "https://api.openai.com",
                Some("sk"),
                &["gpt-5.1".to_string()],
            )
            .unwrap();
        connections
            .add(
                "ollama",
                "http://localhost:11434",
                None,
                &["qwen3:8b".to_string()],
            )
            .unwrap();
        settings.set(ON_FAILURE_KEY, "fallback").unwrap();
        settings
            .set(FALLBACK_CHAIN_KEY, r#"["qwen3:8b","gpt-5.1"]"#)
            .unwrap();

        let targets = resolve_fallback_targets(&connections, &settings, &secrets)
            .expect("resolution should succeed");

        assert_eq!(targets.len(), 2);
        // Order is preserved (Ollama first, as configured), and each target
        // carries its vendor-native provider.
        assert_eq!(targets[0].model, "qwen3:8b");
        assert_eq!(
            targets[0].provider.provider_name(),
            llm_provider::OllamaProvider::new("x").provider_name()
        );
        assert_eq!(targets[1].model, "gpt-5.1");
    }

    #[test]
    fn resolve_fallback_targets_is_empty_unless_on_failure_is_fallback() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("resolve-fallback-notify");
        connections
            .add(
                "openai",
                "https://api.openai.com",
                Some("sk"),
                &["gpt-5.1".to_string()],
            )
            .unwrap();
        // A chain is configured, but on-failure is the default "notify".
        settings.set(ON_FAILURE_KEY, "notify").unwrap();
        settings.set(FALLBACK_CHAIN_KEY, r#"["gpt-5.1"]"#).unwrap();

        let targets = resolve_fallback_targets(&connections, &settings, &secrets).unwrap();
        assert!(targets.is_empty(), "notify mode must not build a fallback chain");
    }

    #[test]
    fn resolve_fallback_targets_skips_ids_no_enabled_connection_has() {
        let connections = new_connections();
        let settings = new_settings();
        let (secrets, _dir) = new_secrets("resolve-fallback-skip");
        connections
            .add(
                "openai",
                "https://api.openai.com",
                Some("sk"),
                &["gpt-5.1".to_string()],
            )
            .unwrap();
        settings.set(ON_FAILURE_KEY, "fallback").unwrap();
        // "no-such-model" matches no enabled connection -> skipped.
        settings
            .set(FALLBACK_CHAIN_KEY, r#"["no-such-model","gpt-5.1"]"#)
            .unwrap();

        let targets = resolve_fallback_targets(&connections, &settings, &secrets).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].model, "gpt-5.1");
    }

    // ---- quote_mode_from_settings ----

    #[test]
    fn quote_mode_from_settings_defaults_when_unset() {
        let settings = new_settings();
        assert_eq!(
            quote_mode_from_settings(&settings),
            Ok(QuoteMode::default())
        );
    }

    #[test]
    fn quote_mode_from_settings_reads_each_configured_value() {
        let settings = new_settings();
        settings.set(QUOTE_MODE_KEY, "answer").unwrap();
        assert_eq!(quote_mode_from_settings(&settings), Ok(QuoteMode::AnswerOnly));
        settings.set(QUOTE_MODE_KEY, "answer_quote").unwrap();
        assert_eq!(
            quote_mode_from_settings(&settings),
            Ok(QuoteMode::IncludeQuote)
        );
        settings.set(QUOTE_MODE_KEY, "rd").unwrap();
        assert_eq!(
            quote_mode_from_settings(&settings),
            Ok(QuoteMode::LetDirectionDecide)
        );
    }

    // ---- inject_mode_from_settings ----

    #[test]
    fn inject_mode_from_settings_defaults_to_blind_when_unset() {
        let settings = SettingsStore::open_in_memory().unwrap();
        assert_eq!(inject_mode_from_settings(&settings), Ok(InjectMode::Blind));
    }

    #[test]
    fn inject_mode_from_settings_reads_review_when_configured() {
        let settings = SettingsStore::open_in_memory().unwrap();
        settings.set(INJECT_MODE_KEY, "review").unwrap();
        assert_eq!(inject_mode_from_settings(&settings), Ok(InjectMode::Review));
    }

    // ---- paused gate (is_paused / set_paused / run_refine) ----

    #[test]
    fn is_paused_defaults_to_false_when_unset() {
        let settings = SettingsStore::open_in_memory().unwrap();
        assert!(!is_paused(&settings));
    }

    #[test]
    fn set_paused_then_is_paused_round_trips() {
        let settings = SettingsStore::open_in_memory().unwrap();

        set_paused(&settings, true).unwrap();
        assert!(is_paused(&settings));

        set_paused(&settings, false).unwrap();
        assert!(!is_paused(&settings));
    }

    // ---- run_refine_with (the pipeline `dispatch_hotkey`/`refine`/
    // `tray_refine` all funnel through) ----
    //
    // Exercised directly against in-memory stores -- no `tauri::App` needed
    // (`run_refine`, the `AppHandle`-unwrapping wrapper around this, is
    // covered transitively: it does nothing but pull these same four stores
    // out of Tauri state).

    #[tokio::test]
    async fn run_refine_with_rejects_with_no_active_model_error_when_nothing_is_configured() {
        let settings = new_settings();
        let connections = new_connections();
        let (secrets, _dir) = new_secrets("run-refine-no-model");
        let refine_state = RefineState::default();

        let result =
            run_refine_with(&settings, &connections, &secrets, &refine_state).await;

        // No stored connection with an enabled model -> `active_provider`
        // rejects with `NO_ACTIVE_MODEL_ERROR`, proving the real pipeline
        // (not a stub) ran.
        assert_eq!(result, Err(NO_ACTIVE_MODEL_ERROR.to_string()));
    }

    /// Boundary test for the B17/B23 paused-gate reconciliation: while
    /// capturing is paused, the pipeline rejects with [`PAUSED_ERROR`]
    /// *before* `active_provider`/`NO_ACTIVE_MODEL_ERROR` -- proving the
    /// gate sits at the top of the one shared pipeline the hotkey, `refine`,
    /// and `tray_refine` all funnel through, not just something
    /// `tray_pause` itself checks client-side.
    #[tokio::test]
    async fn run_refine_with_rejects_with_paused_error_before_checking_for_an_active_model() {
        let settings = new_settings();
        set_paused(&settings, true).unwrap();
        let connections = new_connections();
        let (secrets, _dir) = new_secrets("run-refine-paused");
        let refine_state = RefineState::default();

        let result =
            run_refine_with(&settings, &connections, &secrets, &refine_state).await;

        assert_eq!(result, Err(PAUSED_ERROR.to_string()));
    }

    // ---- should_dispatch_hotkey (dispatch_hotkey's routing decision) ----

    #[test]
    fn should_dispatch_hotkey_ignores_a_release_event() {
        let hotkey_state = HotkeyState::default();
        *hotkey_state.current.lock().unwrap() = Some(hotkey::DEFAULT_HOTKEY.to_string());
        let shortcut = Shortcut::from_str(hotkey::DEFAULT_HOTKEY).unwrap();
        let event = ShortcutEvent {
            id: shortcut.id(),
            state: ShortcutState::Released,
        };

        assert!(
            !should_dispatch_hotkey(&hotkey_state, &shortcut, &event),
            "a key-up event must not trigger refine"
        );
    }

    #[test]
    fn should_dispatch_hotkey_ignores_a_shortcut_that_is_not_the_current_hotkey() {
        let hotkey_state = HotkeyState::default();
        *hotkey_state.current.lock().unwrap() = Some(hotkey::DEFAULT_HOTKEY.to_string());
        // Something other than the currently-registered combo fired -- e.g.
        // a foreign shortcut sharing the plugin's one global handler.
        let other = Shortcut::from_str("Ctrl+Alt+T").unwrap();
        let event = ShortcutEvent {
            id: other.id(),
            state: ShortcutState::Pressed,
        };

        assert!(
            !should_dispatch_hotkey(&hotkey_state, &other, &event),
            "a shortcut other than the current hotkey must not trigger refine"
        );
    }

    #[test]
    fn should_dispatch_hotkey_ignores_events_when_no_hotkey_is_registered_yet() {
        let hotkey_state = HotkeyState::default();
        let shortcut = Shortcut::from_str(hotkey::DEFAULT_HOTKEY).unwrap();
        let event = ShortcutEvent {
            id: shortcut.id(),
            state: ShortcutState::Pressed,
        };

        assert!(!should_dispatch_hotkey(&hotkey_state, &shortcut, &event));
    }

    #[test]
    fn should_dispatch_hotkey_is_true_for_a_matching_key_down() {
        let hotkey_state = HotkeyState::default();
        *hotkey_state.current.lock().unwrap() = Some(hotkey::DEFAULT_HOTKEY.to_string());
        let shortcut = Shortcut::from_str(hotkey::DEFAULT_HOTKEY).unwrap();
        let event = ShortcutEvent {
            id: shortcut.id(),
            state: ShortcutState::Pressed,
        };

        assert!(should_dispatch_hotkey(&hotkey_state, &shortcut, &event));
    }
}
