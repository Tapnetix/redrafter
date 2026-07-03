// Menu-bar tray (wireframes/tray.html, controls/tray.json).
//
// A9 shipped the skeleton: a status icon plus Refine (`tray_refine`) and
// Quit (`tray_quit`). B23 builds the real menu the manifest calls for: the
// active-model switcher (favorites, then every enabled model grouped by
// provider — mirrors `Tray.tsx`'s `renderTrayModels`-equivalent B9 already
// built on the frontend side), Pause/Resume, Settings…/History…, Check for
// updates…, Launch at login, and Quit — plus a status-reflecting tooltip
// (idle/refining/paused/error).
//
// The tray icon itself (id "main") is created by Tauri from
// `tauri.conf.json`'s `app.trayIcon` block; this module only attaches the
// menu/tooltip/event handlers to it once the app is running, and rebuilds
// the menu whenever tray-relevant state changes (a model is picked,
// capturing is paused/resumed) so a reopened dropdown reflects it.

use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    Manager, Runtime,
};

use crate::connections::ConnectionStore;
use crate::models::{self, CuratedModel, ModelsListResult};
use crate::settings::SettingsStore;

/// Settings key `tray_set_launch_login`/the native "Launch at login" item
/// persist the preference under. Unset defaults to `true` (mirrors
/// `Tray.tsx`'s own default), matching the plan's "on by default, opt out"
/// framing for a capture-utility app.
const LAUNCH_AT_LOGIN_SETTING_KEY: &str = "launch_at_login";

/// Prefix for a model-pick menu item's id, followed by a JSON-encoded
/// `(connection_id, model_id)` pair (mirrors `models.rs`'s own `model_key`)
/// rather than a delimited string — a bare `:`-join would be ambiguous for
/// a model id that itself contains `:` (e.g. Ollama's `qwen3:8b`).
const MODEL_MENU_ID_PREFIX: &str = "model:";

fn model_menu_id(model: &CuratedModel) -> String {
    format!(
        "{MODEL_MENU_ID_PREFIX}{}",
        serde_json::to_string(&(&model.connection_id, &model.model_id)).unwrap_or_default()
    )
}

/// Inverse of [`model_menu_id`]: `None` for anything that isn't a
/// `model:`-prefixed id, or whose payload doesn't decode.
fn parse_model_menu_id(id: &str) -> Option<(String, String)> {
    let payload = id.strip_prefix(MODEL_MENU_ID_PREFIX)?;
    serde_json::from_str(payload).ok()
}

/// Attaches the tray menu and a ready tooltip to the tray icon declared in
/// `tauri.conf.json`, and registers the click handler. Logs (rather than
/// errors out) when the icon isn't found so a headless/CI build without a
/// real tray backend doesn't fail app setup.
///
/// Not unit-tested directly: on Linux, building a native menu (even without
/// a tray icon attached to it) needs a running GTK main loop, which only
/// exists once the real app has called `Builder::run` — a plain `#[test]`
/// under `tauri::test::MockRuntime` panics with "GTK has not been
/// initialized" before this function's first line even returns. This is the
/// "real tray icon attach" glue the coverage gate allows as an exception;
/// [`handle_menu_event`]/[`rebuild_tray_menu`]'s branch logic and the
/// command wrappers below are extracted specifically so they *are*
/// unit-testable (mirroring the rest of this file's existing pattern).
pub fn setup_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.on_menu_event(move |app, event| handle_menu_event(app, event.id().as_ref()));
    } else {
        eprintln!("[tray] tray icon 'main' not found — tray menu not configured");
    }
    refresh_tray(app);
    Ok(())
}

/// Rebuilds the tray's menu (active-model switcher, paused/resume item,
/// refine enabled/disabled) and refreshes its "Ready"/"Paused" tooltip from
/// current state. A no-op when there's no real tray icon (`tray_by_id`
/// returns `None` under `tauri::test::MockRuntime` — see [`setup_tray`]'s
/// doc note on why menu construction can't run under tests at all), so it's
/// safe to call from every command that changes tray-relevant state
/// without special-casing test builds. Best-effort: logs rather than
/// propagates a build failure, matching [`setup_tray`].
pub(crate) fn refresh_tray<R: Runtime>(app: &tauri::AppHandle<R>) {
    if app.tray_by_id("main").is_none() {
        return;
    }
    if let Err(e) = rebuild_tray_menu(app) {
        eprintln!("[tray] failed to refresh tray menu: {e}");
    }
}

/// Sets a transient status message on the tray tooltip, distinct from
/// [`refresh_tray`]'s idle/paused recompute — currently the update-check
/// flow's "Update available…"/"Ready" result (see [`check_for_updates`]).
/// A no-op when there's no real tray icon. Only touches the tooltip (not a
/// GTK menu), but the in-flight "Refining…" reflection stays on the
/// frontend `Tray.tsx` surface for now; wiring it into the shared
/// `run_refine` pipeline would couple that pipeline (exercised by the
/// hotkey/refine unit tests) to the native tray, deferred to the macOS
/// tray pass.
pub(crate) fn set_status_message<R: Runtime>(app: &tauri::AppHandle<R>, message: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("redrafter — {message}")));
    }
}

/// Which section of the active-model switcher a [`TrayModelRow`] falls in,
/// mirroring `rebuild_tray_menu`'s two-part layout: the flat favorites list
/// first, then every model grouped by provider (first-seen order). A
/// favorite still gets its own row in the provider section too -- the menu
/// lists it twice, same as before this was extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraySection {
    Favorites,
    Provider,
}

/// A single row `rebuild_tray_menu` turns into a `CheckMenuItemBuilder` for
/// the active-model switcher submenu, in final display order.
#[derive(Debug, Clone, PartialEq)]
struct TrayModelRow {
    model: CuratedModel,
    section: TraySection,
}

/// Pure ordering decision behind the tray's active-model switcher: favorites
/// first (in `models` order), then every enabled model grouped by provider
/// in first-seen order (mirrors `Tray.tsx`'s own grouping). Extracted out of
/// `rebuild_tray_menu`'s `CheckMenuItemBuilder`/`SubmenuBuilder` glue (which
/// needs a live GTK loop, see `setup_tray`'s doc note) so this decision is
/// unit-testable on its own.
fn tray_model_rows(result: &ModelsListResult) -> Vec<TrayModelRow> {
    let mut rows = Vec::new();

    for model in result.models.iter().filter(|m| m.favorite) {
        rows.push(TrayModelRow {
            model: model.clone(),
            section: TraySection::Favorites,
        });
    }

    // Providers in first-seen order (mirrors `Tray.tsx`'s own grouping), so
    // the full per-connection list beneath favorites is stable rather than
    // resorting on every rebuild.
    let mut providers: Vec<&str> = Vec::new();
    for model in &result.models {
        if !providers.contains(&model.provider_kind.as_str()) {
            providers.push(&model.provider_kind);
        }
    }
    for provider in &providers {
        for model in result.models.iter().filter(|m| m.provider_kind == *provider) {
            rows.push(TrayModelRow {
                model: model.clone(),
                section: TraySection::Provider,
            });
        }
    }

    rows
}

/// The active-model switcher submenu's title suffix ("Active model: <id>",
/// or a placeholder when nothing is active) -- pure text extracted out of
/// `rebuild_tray_menu` so it's unit-testable.
fn tray_active_model_label(result: &ModelsListResult) -> String {
    result
        .models
        .iter()
        .find(|m| m.active)
        .map(|m| m.model_id.clone())
        .unwrap_or_else(|| "No model selected".to_string())
}

fn rebuild_tray_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else {
        return Ok(());
    };

    let connections = app.state::<ConnectionStore>();
    let settings = app.state::<SettingsStore>();
    let result = models::models_list_impl(&connections, &settings).unwrap_or(ModelsListResult {
        models: Vec::new(),
        has_active: false,
        active_unavailable: false,
        stale_active_model_id: None,
    });
    let paused = crate::is_paused(&settings);
    let launch_at_login = settings
        .get(LAUNCH_AT_LOGIN_SETTING_KEY)
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);

    let refine_item = MenuItemBuilder::with_id("refine", "Refine selection")
        .enabled(!paused)
        .build(app)?;

    let active_label = tray_active_model_label(&result);
    let mut switcher_builder = SubmenuBuilder::new(app, format!("Active model: {active_label}"));

    let mut previous_section: Option<TraySection> = None;
    for row in tray_model_rows(&result) {
        // A separator between the favorites list and the grouped-by-provider
        // list below it, but only once, and only when favorites came first.
        if previous_section == Some(TraySection::Favorites) && row.section == TraySection::Provider
        {
            switcher_builder = switcher_builder.separator();
        }
        let item = CheckMenuItemBuilder::with_id(model_menu_id(&row.model), &row.model.model_id)
            .checked(row.model.active)
            .build(app)?;
        switcher_builder = switcher_builder.item(&item);
        previous_section = Some(row.section);
    }
    let manage_models = MenuItemBuilder::with_id("manage-models", "Manage models…").build(app)?;
    switcher_builder = switcher_builder.separator().item(&manage_models);
    let switcher = switcher_builder.build()?;

    let pause_resume = if paused {
        MenuItemBuilder::with_id("resume", "Resume capturing").build(app)?
    } else {
        MenuItemBuilder::with_id("pause", "Pause capturing").build(app)?
    };

    let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let history_item = MenuItemBuilder::with_id("history", "History…").build(app)?;
    let updates_item =
        MenuItemBuilder::with_id("check-updates", "Check for updates…").build(app)?;
    let launch_login_item = CheckMenuItemBuilder::with_id("launch-login", "Launch at login")
        .checked(launch_at_login)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit redrafter").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&refine_item)
        .separator()
        .item(&switcher)
        .separator()
        .item(&pause_resume)
        .separator()
        .item(&settings_item)
        .item(&history_item)
        .item(&updates_item)
        .item(&launch_login_item)
        .separator()
        .item(&quit)
        .build()?;

    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(if paused {
        "redrafter — Paused"
    } else {
        "redrafter — Ready"
    }))?;
    Ok(())
}

/// The action a tray menu-item id maps to. Separating the *routing* (id ->
/// action, a pure decision) from *applying* it (`handle_menu_event`, which
/// touches the OS tray/plugins) is what lets the routing be unit-tested on
/// Linux without a real GTK tray — building any native menu under
/// `tauri::test::MockRuntime` panics with "GTK has not been initialized"
/// (the known tray SIGSEGV), so anything downstream of `refresh_tray` can't
/// run headlessly. See `menu_action_for`'s tests below.
#[derive(Debug, Clone, PartialEq)]
enum MenuAction {
    Refine,
    SetActiveModel { connection_id: String, model_id: String },
    Pause,
    Resume,
    CheckUpdates,
    ToggleLaunchLogin,
    Quit,
    /// A non-actionable id ("manage-models"/"settings"/"history", or an
    /// unknown one): no native-window routing target exists yet (the app
    /// shell's section switcher lives entirely in the frontend, `App.tsx`),
    /// so these are best-effort no-ops, matching this task's "Linux
    /// best-effort" carve-out.
    Ignore,
}

/// Pure router from a tray menu-item id to the [`MenuAction`] it triggers.
/// Unit-testable independent of the OS tray (see [`MenuAction`]).
fn menu_action_for(id: &str) -> MenuAction {
    if let Some((connection_id, model_id)) = parse_model_menu_id(id) {
        return MenuAction::SetActiveModel {
            connection_id,
            model_id,
        };
    }
    match id {
        "refine" => MenuAction::Refine,
        "pause" => MenuAction::Pause,
        "resume" => MenuAction::Resume,
        "check-updates" => MenuAction::CheckUpdates,
        "launch-login" => MenuAction::ToggleLaunchLogin,
        "quit" => MenuAction::Quit,
        _ => MenuAction::Ignore,
    }
}

/// Reacts to a tray menu selection: routes the id via [`menu_action_for`]
/// then applies the action. Pulled out of `setup_tray`'s `on_menu_event`
/// closure. The apply half touches the native tray (`refresh_tray`) and the
/// updater/autostart plugins, none of which run under
/// `tauri::test::MockRuntime` on Linux (see [`MenuAction`]/`setup_tray`) —
/// so it's untestable glue, same carve-out as `setup_tray`'s menu
/// construction; the *routing* it depends on is covered by
/// `menu_action_for`'s tests, and the state each action ultimately mutates
/// by `models`/`lib`/`set_launch_at_login_impl`'s own tests.
fn handle_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    match menu_action_for(id) {
        MenuAction::Refine => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::run_refine(&handle).await;
            });
        }
        MenuAction::SetActiveModel {
            connection_id,
            model_id,
        } => {
            let connections = app.state::<ConnectionStore>();
            let settings = app.state::<SettingsStore>();
            let _ =
                models::model_set_active_impl(&connections, &settings, &connection_id, &model_id);
            refresh_tray(app);
        }
        MenuAction::Pause => {
            let settings = app.state::<SettingsStore>();
            let _ = crate::set_paused(&settings, true);
            refresh_tray(app);
        }
        MenuAction::Resume => {
            let settings = app.state::<SettingsStore>();
            let _ = crate::set_paused(&settings, false);
            refresh_tray(app);
        }
        MenuAction::CheckUpdates => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = check_for_updates(&handle).await {
                    eprintln!("[tray] update check failed: {e}");
                }
            });
        }
        MenuAction::ToggleLaunchLogin => {
            let settings = app.state::<SettingsStore>();
            let current = settings
                .get(LAUNCH_AT_LOGIN_SETTING_KEY)
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);
            let controller = RealLaunchAtLogin(app.clone());
            if let Err(e) = set_launch_at_login_impl(&controller, &settings, !current) {
                eprintln!("[tray] failed to toggle launch at login: {e}");
            }
            refresh_tray(app);
        }
        MenuAction::Quit => app.exit(0),
        MenuAction::Ignore => {}
    }
}

/// Runs a real update check (`tauri-plugin-updater`) and refreshes the tray
/// with the result. Split out from `handle_menu_event`'s `"check-updates"`
/// arm only so the async body isn't inlined in a `match` arm; still not
/// unit-tested (see `handle_menu_event`'s doc note) since the plugin state
/// it needs isn't managed under `MockRuntime`.
async fn check_for_updates<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    let message = match update {
        Some(update) => format!("Update available: {}", update.version),
        None => "Ready".to_string(),
    };
    set_status_message(app, &message);
    Ok(())
}

/// Tauri command: triggers the same default refine pipeline as the
/// frontend's `refine` command, from the tray's Refine entry. Registered
/// by A14 (`app/src-tauri/src/lib.rs`).
#[tauri::command]
pub async fn tray_refine<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::orchestrator::RefineFlow, String> {
    crate::run_refine(&app).await
}

/// Tauri command: quits the app from the tray's Quit entry (and the
/// Capture panel's display-only tray preview, per its carve-out).
/// Registered by A14 (`app/src-tauri/src/lib.rs`).
#[tauri::command]
pub fn tray_quit<R: Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
}

/// Tauri command: sets the active model from the menu-bar tray's
/// quick-switch (`Tray.tsx`, B9) -- distinct from the Models screen's
/// `model_set_active` per `controls/tray.json`, but the same guarded logic
/// underneath (rejects a model that isn't enabled). Also refreshes the
/// native tray menu so a reopened dropdown reflects the new pick.
#[tauri::command]
pub fn tray_set_active_model<R: Runtime>(
    app: tauri::AppHandle<R>,
    connections: tauri::State<'_, ConnectionStore>,
    settings: tauri::State<'_, SettingsStore>,
    connection_id: String,
    model_id: String,
) -> Result<ModelsListResult, String> {
    let result =
        models::model_set_active_impl(&connections, &settings, &connection_id, &model_id)?;
    refresh_tray(&app);
    Ok(result)
}

/// Tauri command: pauses global capturing -- both the global hotkey and the
/// tray's own "Refine selection" stop triggering a refine (`run_refine`'s
/// paused gate, `lib.rs`) until `tray_resume`. Persists the flag via
/// `crate::set_paused` (B17/B23) so it survives a tray reopen (and a
/// restart), and refreshes the native menu/tooltip.
#[tauri::command]
pub fn tray_pause<R: Runtime>(
    app: tauri::AppHandle<R>,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<(), String> {
    crate::set_paused(&settings, true)?;
    refresh_tray(&app);
    Ok(())
}

/// Tauri command: resumes global capturing after [`tray_pause`].
#[tauri::command]
pub fn tray_resume<R: Runtime>(
    app: tauri::AppHandle<R>,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<(), String> {
    crate::set_paused(&settings, false)?;
    refresh_tray(&app);
    Ok(())
}

/// Result of a `tray_check_updates` call: whether a newer version is
/// available, and which one. Mirrors `ipc.ts`'s `CheckUpdatesResult`
/// (`#[serde(rename_all = "camelCase")]`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdatesResult {
    pub update_available: bool,
    pub version: Option<String>,
}

/// Maps whether an update is available (and, if so, its version) into the
/// wire shape `tray_check_updates` resolves with. Pulled out so this
/// mapping is unit-testable without a live updater-endpoint network call --
/// see `handle_menu_event`'s doc note for why the command itself isn't.
fn check_updates_result(available_version: Option<String>) -> CheckUpdatesResult {
    match available_version {
        Some(version) => CheckUpdatesResult {
            update_available: true,
            version: Some(version),
        },
        None => CheckUpdatesResult {
            update_available: false,
            version: None,
        },
    }
}

/// Tauri command: triggers an application-update check from the tray/the
/// in-app tray preview (`Tray.tsx`), via `tauri-plugin-updater`.
#[tauri::command]
pub async fn tray_check_updates<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<CheckUpdatesResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(check_updates_result(update.map(|u| u.version)))
}

/// The seam behind [`tray_set_launch_login`]'s real `tauri-plugin-autostart`
/// call, so [`set_launch_at_login_impl`] (the settings-persistence half of
/// the command) is unit-testable with a fake, independent of the plugin's
/// managed state (unavailable under `tauri::test::MockRuntime` -- see
/// `handle_menu_event`'s doc note).
trait LaunchAtLoginController {
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
}

/// Production [`LaunchAtLoginController`], backed by
/// `tauri_plugin_autostart`'s real `AutoLaunchManager`.
struct RealLaunchAtLogin<R: Runtime>(tauri::AppHandle<R>);

impl<R: Runtime> LaunchAtLoginController for RealLaunchAtLogin<R> {
    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        use tauri_plugin_autostart::ManagerExt;
        let manager = self.0.autolaunch();
        let result = if enabled {
            manager.enable()
        } else {
            manager.disable()
        };
        result.map_err(|e| e.to_string())
    }
}

/// Toggles launch-at-login through `controller` and persists the choice.
/// Generic over [`LaunchAtLoginController`] (mirrors `permission_gate`'s
/// `AccessibilityChecker` pattern in `lib.rs`) so this is unit-testable with
/// a fake instead of the real OS-level autostart mechanism.
fn set_launch_at_login_impl<C: LaunchAtLoginController>(
    controller: &C,
    settings: &SettingsStore,
    enabled: bool,
) -> Result<(), String> {
    controller.set_enabled(enabled)?;
    settings
        .set(LAUNCH_AT_LOGIN_SETTING_KEY, if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

/// Tauri command: toggles whether redrafter launches automatically at
/// login (`Tray.tsx`'s "Launch at login"), persisting the preference and
/// driving the real `tauri-plugin-autostart` (see [`RealLaunchAtLogin`]).
#[tauri::command]
pub fn tray_set_launch_login<R: Runtime>(
    app: tauri::AppHandle<R>,
    settings: tauri::State<'_, SettingsStore>,
    enabled: bool,
) -> Result<(), String> {
    let controller = RealLaunchAtLogin(app.clone());
    set_launch_at_login_impl(&controller, &settings, enabled)?;
    refresh_tray(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::ConnectionStore;
    use crate::settings::SettingsStore;
    use crate::RefineState;
    use tauri::Manager;

    /// Builds a `MockRuntime` app with every store `run_refine` (transitively
    /// reached by `tray_refine`/`handle_menu_event`'s "refine" branch) needs
    /// managed, exactly like `tests/wireup_test.rs`'s `build_test_app`. No
    /// real window/tray backend — `tray_icon` is always `None` under
    /// `mock_builder`, which is exactly the "tray icon not found" fallback
    /// `setup_tray`/`refresh_tray` are built to handle.
    fn managed_app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::generate_context!())
            .expect("failed to build mock app");
        app.manage(SettingsStore::open_in_memory().expect("failed to open in-memory settings"));
        app.manage(ConnectionStore::open_in_memory().expect("failed to open in-memory connections"));
        app.manage(crate::secrets::SecretStore::open(&secrets_temp_dir()).expect("failed to open secret store"));
        app.manage(RefineState::default());
        app
    }

    /// A process/thread-unique temp dir for a `SecretStore` in tests
    /// (mirrors the same pattern in `lib.rs`/`connections.rs`/`secrets.rs`).
    fn secrets_temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "redrafter_tray_secrets_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn handle_menu_event_quits_on_the_quit_id() {
        // `AppHandle::exit` -> `request_exit` is `unimplemented!()` under
        // `MockRuntime` (there's no real event loop to deliver
        // `RunEvent::Exit` to) — genuinely untestable platform glue, same as
        // `setup_tray`'s native menu construction. `catch_unwind` still lets
        // this assert the "quit" arm actually reaches and calls `exit`
        // (the panic happens one level down, inside tauri's `AppHandle::exit`,
        // not before it), without crashing the test process.
        let app = managed_app();
        let handle = app.handle().clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle_menu_event(&handle, "quit");
        }));
        assert!(result.is_err(), "expected the 'quit' arm to call AppHandle::exit");
    }

    #[test]
    fn handle_menu_event_ignores_an_unknown_id() {
        let app = managed_app();
        // Must be a no-op: no panic, no exit, no refine spawned.
        handle_menu_event(&app.handle().clone(), "something-else");
    }

    #[test]
    fn handle_menu_event_spawns_a_refine_on_the_refine_id() {
        let app = managed_app();
        // Spawning must not panic even though the spawned task itself will
        // fail fast (no stored connection -> `NO_ACTIVE_MODEL_ERROR`, or a
        // real capture failure off macOS) — `handle_menu_event` only fires
        // the pipeline and forgets the result, mirroring `tray_refine`.
        // (The "refine" arm never calls `refresh_tray`, so no GTK menu is
        // built -- unlike the pause/resume/model-pick arms, whose state
        // effects are covered by `menu_action_for` routing tests plus
        // `set_paused`/`model_set_active_impl`'s own tests, since applying
        // them here would panic building a native menu under `MockRuntime`.)
        handle_menu_event(&app.handle().clone(), "refine");
    }

    // ── tray_model_rows / tray_active_model_label (pure ordering, GTK-free) ──

    fn model(connection_id: &str, model_id: &str, provider_kind: &str, active: bool, favorite: bool) -> CuratedModel {
        CuratedModel {
            connection_id: connection_id.to_string(),
            model_id: model_id.to_string(),
            provider_kind: provider_kind.to_string(),
            active,
            favorite,
        }
    }

    fn models_result(models: Vec<CuratedModel>) -> ModelsListResult {
        ModelsListResult {
            models,
            has_active: false,
            active_unavailable: false,
            stale_active_model_id: None,
        }
    }

    #[test]
    fn tray_model_rows_is_empty_for_no_models() {
        assert_eq!(tray_model_rows(&models_result(Vec::new())), Vec::new());
    }

    #[test]
    fn tray_model_rows_lists_favorites_first_then_every_model_grouped_by_provider() {
        let result = models_result(vec![
            model("1", "claude-opus", "anthropic", false, false),
            model("2", "qwen3:8b", "ollama", false, true),
            model("1", "claude-haiku", "anthropic", true, false),
        ]);

        let rows = tray_model_rows(&result);

        // Favorites first (in `models` order), then every model grouped by
        // provider in first-seen order -- so the one favorite comes first,
        // then anthropic's two models (first-seen provider), then ollama's.
        assert_eq!(
            rows,
            vec![
                TrayModelRow {
                    model: model("2", "qwen3:8b", "ollama", false, true),
                    section: TraySection::Favorites,
                },
                TrayModelRow {
                    model: model("1", "claude-opus", "anthropic", false, false),
                    section: TraySection::Provider,
                },
                TrayModelRow {
                    model: model("1", "claude-haiku", "anthropic", true, false),
                    section: TraySection::Provider,
                },
                TrayModelRow {
                    model: model("2", "qwen3:8b", "ollama", false, true),
                    section: TraySection::Provider,
                },
            ]
        );
    }

    #[test]
    fn tray_model_rows_omits_the_favorites_section_when_there_are_none() {
        let result = models_result(vec![model("1", "claude-opus", "anthropic", false, false)]);

        let rows = tray_model_rows(&result);

        assert!(rows.iter().all(|r| r.section == TraySection::Provider));
    }

    #[test]
    fn tray_model_rows_flags_the_active_model_in_every_section_it_appears_in() {
        let result = models_result(vec![model("1", "claude-opus", "anthropic", true, true)]);

        let rows = tray_model_rows(&result);

        assert!(rows.iter().all(|r| r.model.active));
    }

    #[test]
    fn tray_active_model_label_reports_the_active_models_id() {
        let result = models_result(vec![
            model("1", "claude-opus", "anthropic", false, false),
            model("1", "claude-haiku", "anthropic", true, false),
        ]);

        assert_eq!(tray_active_model_label(&result), "claude-haiku");
    }

    #[test]
    fn tray_active_model_label_reports_a_placeholder_when_nothing_is_active() {
        assert_eq!(
            tray_active_model_label(&models_result(Vec::new())),
            "No model selected"
        );
    }

    // ── menu_action_for (pure routing, GTK-free) ──

    #[test]
    fn menu_action_for_routes_the_fixed_ids() {
        assert_eq!(menu_action_for("refine"), MenuAction::Refine);
        assert_eq!(menu_action_for("pause"), MenuAction::Pause);
        assert_eq!(menu_action_for("resume"), MenuAction::Resume);
        assert_eq!(menu_action_for("check-updates"), MenuAction::CheckUpdates);
        assert_eq!(menu_action_for("launch-login"), MenuAction::ToggleLaunchLogin);
        assert_eq!(menu_action_for("quit"), MenuAction::Quit);
    }

    #[test]
    fn menu_action_for_routes_a_non_actionable_id_to_ignore() {
        assert_eq!(menu_action_for("manage-models"), MenuAction::Ignore);
        assert_eq!(menu_action_for("settings"), MenuAction::Ignore);
        assert_eq!(menu_action_for("history"), MenuAction::Ignore);
        assert_eq!(menu_action_for("something-else"), MenuAction::Ignore);
    }

    #[test]
    fn menu_action_for_routes_a_model_pick_id_to_set_active_model() {
        let model = CuratedModel {
            connection_id: "7".to_string(),
            model_id: "qwen3:8b".to_string(),
            provider_kind: "ollama".to_string(),
            active: false,
            favorite: false,
        };

        assert_eq!(
            menu_action_for(&model_menu_id(&model)),
            MenuAction::SetActiveModel {
                connection_id: "7".to_string(),
                model_id: "qwen3:8b".to_string(),
            }
        );
    }

    #[test]
    fn parse_model_menu_id_round_trips_a_model_id_containing_a_colon() {
        let model = CuratedModel {
            connection_id: "1".to_string(),
            model_id: "qwen3:8b".to_string(),
            provider_kind: "ollama".to_string(),
            active: false,
            favorite: false,
        };
        let id = model_menu_id(&model);
        assert_eq!(
            parse_model_menu_id(&id),
            Some(("1".to_string(), "qwen3:8b".to_string()))
        );
    }

    #[test]
    fn parse_model_menu_id_rejects_a_non_model_id() {
        assert_eq!(parse_model_menu_id("refine"), None);
        assert_eq!(parse_model_menu_id("quit"), None);
    }

    #[tokio::test]
    async fn tray_refine_propagates_the_pipeline_result() {
        let app = managed_app();
        // No stored connection with an enabled model -> `run_refine` rejects
        // with `NO_ACTIVE_MODEL_ERROR` before ever touching the network.
        let result = tray_refine(app.handle().clone()).await;
        assert_eq!(result, Err(crate::NO_ACTIVE_MODEL_ERROR.to_string()));
    }

    #[test]
    fn tray_quit_exits_the_app() {
        // See `handle_menu_event_quits_on_the_quit_id` above: `AppHandle::exit`
        // is unimplemented under `MockRuntime`, so this only asserts the
        // command actually reaches and calls it.
        let app = managed_app();
        let handle = app.handle().clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tray_quit(handle);
        }));
        assert!(result.is_err(), "expected tray_quit to call AppHandle::exit");
    }

    // ---- tray_set_active_model ----
    //
    // `tray_set_active_model`/`tray_pause`/`tray_resume`'s *happy* paths all
    // end in `refresh_tray`, which builds a native GTK menu that panics
    // under `MockRuntime` on Linux (the known tray SIGSEGV) — so those are
    // untestable glue here (their state effect is covered by
    // `model_set_active_impl`/`set_paused`'s own tests, and end-to-end by
    // the `tray_set_active_model`/`tray_pause`/`tray_resume` e2e specs).
    // The *rejection* path below returns before `refresh_tray`, so it stays
    // unit-testable.

    #[test]
    fn tray_set_active_model_rejects_a_model_that_isnt_enabled() {
        let app = managed_app();
        let connections = app.state::<ConnectionStore>();
        let added = connections
            .add("anthropic", "https://api.anthropic.com", Some("sk"), &[])
            .unwrap();

        let err = tray_set_active_model(
            app.handle().clone(),
            app.state(),
            app.state(),
            added.id,
            "claude-opus-4-6".to_string(),
        )
        .unwrap_err();

        assert!(err.contains("not enabled"), "got: {err}");
    }

    // ---- check_updates_result ----

    #[test]
    fn check_updates_result_reports_no_update_for_none() {
        assert_eq!(
            check_updates_result(None),
            CheckUpdatesResult {
                update_available: false,
                version: None,
            }
        );
    }

    #[test]
    fn check_updates_result_reports_the_available_version() {
        assert_eq!(
            check_updates_result(Some("1.2.3".to_string())),
            CheckUpdatesResult {
                update_available: true,
                version: Some("1.2.3".to_string()),
            }
        );
    }

    // ---- set_launch_at_login_impl (fake controller) ----

    #[derive(Default)]
    struct FakeLaunchController {
        calls: std::sync::Mutex<Vec<bool>>,
        fail: bool,
    }

    impl LaunchAtLoginController for FakeLaunchController {
        fn set_enabled(&self, enabled: bool) -> Result<(), String> {
            if self.fail {
                return Err("simulated autostart failure".to_string());
            }
            self.calls.lock().unwrap().push(enabled);
            Ok(())
        }
    }

    #[test]
    fn set_launch_at_login_impl_calls_the_controller_and_persists_the_choice() {
        let settings = SettingsStore::open_in_memory().unwrap();
        let controller = FakeLaunchController::default();

        set_launch_at_login_impl(&controller, &settings, false).unwrap();

        assert_eq!(*controller.calls.lock().unwrap(), vec![false]);
        assert_eq!(
            settings.get(LAUNCH_AT_LOGIN_SETTING_KEY).unwrap(),
            Some("false".to_string())
        );
    }

    #[test]
    fn set_launch_at_login_impl_does_not_persist_when_the_controller_fails() {
        let settings = SettingsStore::open_in_memory().unwrap();
        let controller = FakeLaunchController {
            fail: true,
            ..Default::default()
        };

        let err = set_launch_at_login_impl(&controller, &settings, true).unwrap_err();

        assert!(err.contains("simulated"), "got: {err}");
        assert_eq!(settings.get(LAUNCH_AT_LOGIN_SETTING_KEY).unwrap(), None);
    }
}
