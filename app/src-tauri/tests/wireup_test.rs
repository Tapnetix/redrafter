//! Exercises the production invoke handler (`redrafter_lib::invoke_handler`)
//! against a `MockRuntime` app to assert every command — Phase A's plus every
//! Phase B command B23 wires up — is actually registered, rather than just
//! compiled. This is the phase wire-up gate: the classic failure mode this
//! guards against is shipping every module but never adding it to
//! `tauri::generate_handler![...]`, so nothing calls it.
//!
//! It also asserts the ACL half of the same contract: every registered
//! command has a matching `allow-<command>` grant in
//! `capabilities/default.json`. A registered-but-ungranted command is
//! silently denied at runtime ("not allowed"), so the two lists must move
//! together — see `every_registered_command_has_an_acl_grant`.
//!
//! Uses `tauri::test::mock_builder`/`get_ipc_response` (a synthetic
//! `MockRuntime`, no real window/OS integration) so this runs headlessly in
//! CI. A registered command is distinguished from an unregistered one by
//! Tauri's own rejection text for the latter ("Command {cmd} not found");
//! a registered command may still return a domain-level `Err` (e.g. no
//! state managed, or a real platform error on non-macOS) without that ever
//! being mistaken for "not registered".

use redrafter_lib::connections::ConnectionStore;
use redrafter_lib::hotkey::HotkeyState;
use redrafter_lib::secrets::SecretStore;
use redrafter_lib::settings::SettingsStore;
use redrafter_lib::RefineState;
use tauri::test::INVOKE_KEY;
use tauri::webview::InvokeRequest;
use tauri::ipc::CallbackFn;
use tauri::Manager;

/// Every command the composition root must register (per the plan's
/// `done_when`): the Phase A screens' commands plus the tray skeleton's
/// `tray_refine`/`tray_quit` (A14), every Phase B command B23 wires up
/// (connections CRUD/test/refresh, model curation, secrets, and the full
/// tray surface), and every Phase C command C17 wires up (presets
/// CRUD/import/export/resolve, history list/detail/restore/re-refine/copy/
/// clear, and the feedback config get/set). This list is the wire-up gate's
/// source of truth and must match `lib.rs`'s `invoke_handler!`, `build.rs`'s
/// `COMMANDS`, and `capabilities/default.json`'s grants.
const EXPECTED_COMMANDS: &[&str] = &[
    // Phase A (A14)
    "settings_get",
    "settings_set",
    "permission_status",
    "permission_open_settings",
    "hotkey_set",
    "connection_add",
    "connection_list",
    "refine",
    "restore_original",
    "inject_text",
    "cancel_refine",
    "tray_refine",
    "tray_quit",
    // Phase B connections (B7b)
    "connection_edit",
    "connection_remove",
    "connection_test",
    "connection_refresh_models",
    "model_add_manual",
    // Phase B models (B8)
    "models_list",
    "model_set_active",
    "model_disable",
    "model_toggle_favorite",
    "ollama_pull",
    // Phase B secrets (B10)
    "secrets_set",
    "secrets_set_key",
    "secrets_delete",
    // Phase B tray (B9/B17/B23)
    "tray_set_active_model",
    "tray_pause",
    "tray_resume",
    "tray_check_updates",
    "tray_set_launch_login",
    // Phase C presets (C3/C3b/C8/C9/C10, resolution wired by C17)
    "preset_list",
    "preset_save",
    "preset_delete",
    "preset_duplicate",
    "preset_reset_default",
    "preset_export",
    "preset_import",
    "preset_resolve",
    // Phase C history (C4/C12-C15)
    "history_list",
    "history_get",
    "history_restore",
    "history_rerefine",
    "history_copy",
    "history_clear",
    // Phase C feedback (C1/C5)
    "feedback_config_get",
    "feedback_config_set",
    // Opening provider console links in the real browser
    "open_external",
];

fn build_test_app() -> tauri::App<tauri::test::MockRuntime> {
    // Uses the project's real `tauri.conf.json` (via `generate_context!`,
    // same as production `run()`) rather than `tauri::test::mock_context`:
    // the latter's `Resolved::default()` ACL rejects *every* command
    // (including registered ones) with "not allowed", which would make
    // every command in this test look unregistered.
    //
    // The real config now ships `capabilities/default.json` (this diff),
    // granting exactly the Phase A commands above to every window. That
    // capability's `windows: ["*"]` scope still only matches requests whose
    // URL resolves to "local" (Tauri's own asset/custom-protocol origin);
    // this test's synthetic `InvokeRequest` uses a plain
    // `http://tauri.localhost` URL, which the ACL treats as a *different*,
    // non-local origin — so every invocation below is actually rejected with
    // "not allowed" (an ACL denial), not run. That's still enough to
    // discriminate registered-vs-not (see `is_unregistered_rejection`'s doc
    // comment: an ACL denial's text never contains "not found"), but it also
    // means none of these command bodies actually execute here — see
    // `src/lib.rs`'s/`src/tray.rs`'s own `#[cfg(test)]` unit tests for
    // coverage of the command bodies themselves.
    let app = tauri::test::mock_builder()
        .invoke_handler(redrafter_lib::invoke_handler())
        .build(tauri::generate_context!())
        .expect("failed to build test app");

    app.manage(SettingsStore::open_in_memory().expect("failed to open in-memory settings"));
    app.manage(ConnectionStore::open_in_memory().expect("failed to open in-memory connections"));
    let secrets_dir = std::env::temp_dir().join(format!(
        "redrafter_wireup_secrets_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    app.manage(SecretStore::open(&secrets_dir).expect("failed to open secret store"));
    app.manage(HotkeyState::default());
    app.manage(RefineState::default());

    app
}

fn invoke(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: Default::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
}

/// Tauri's rejection text for a command absent from the invoke handler —
/// see `webview::mod::PROCESS_IPC_MESSAGE_FN`'s `Command {command} not
/// found`. Any other error (missing state, a real domain error) means the
/// command *was* found and ran.
fn is_unregistered_rejection(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .map(|s| s.contains("not found"))
        .unwrap_or(false)
}

// Every test below that calls `build_test_app()` needs a real
// `tauri::AppHandle`, which means building an app via
// `tauri::test::mock_builder().build(tauri::generate_context!())` -- the
// same call production `run()` makes. `tauri.conf.json`'s `app.trayIcon`
// block means that `build()` initializes the tray-icon plugin, which
// requires the main thread; on real macOS (not reproducible on Linux) a
// test harness that runs tests off the main thread hits
// `Tray(NotMainThread)` there. Ignored on macOS rather than reworked: this
// file's whole point is exercising the *real* `invoke_handler()`/ACL wiring
// end-to-end (registration + grant, not a unit of command logic), so unlike
// `src/lib.rs`/`src/connections.rs`/`src/secrets.rs`/`src/tray.rs`'s own
// `#[cfg(test)]` units (which were rewritten to test inner functions
// directly against in-memory stores, no app needed), there's no
// app-free-but-equivalent way to assert "the production invoke handler
// actually has this command wired up" -- that assertion is definitionally
// about the built app. Treated as a Linux-CI-only check; the wiring itself
// is verified once per phase on Linux, and the mock-app-free unit suite
// (which does need to be green on both platforms) covers the command
// bodies.

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn every_expected_command_is_registered() {
    let app = build_test_app();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock window");

    for cmd in EXPECTED_COMMANDS {
        if let Err(value) = invoke(&window, cmd) {
            assert!(
                !is_unregistered_rejection(&value),
                "expected `{cmd}` to be registered in the invoke handler, got: {value}"
            );
        }
    }
}

/// The ACL half of the wire-up gate: every registered command must have a
/// matching `allow-<command>` grant in `capabilities/default.json`.
/// Tauri v2 denies any custom command without one at runtime ("not
/// allowed") — a registered-but-ungranted command is a silent failure that
/// no amount of frontend-mock testing would catch, so this pins the two
/// lists together. `allow-<command>` uses the command name with underscores
/// swapped for hyphens (Tauri's autogenerated permission-id convention,
/// see `permissions/autogenerated/*.toml`).
#[test]
fn every_registered_command_has_an_acl_grant() {
    let capabilities = include_str!("../capabilities/default.json");
    let parsed: serde_json::Value =
        serde_json::from_str(capabilities).expect("capabilities/default.json must be valid JSON");
    let permissions: Vec<&str> = parsed["permissions"]
        .as_array()
        .expect("permissions must be an array")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();

    for cmd in EXPECTED_COMMANDS {
        let grant = format!("allow-{}", cmd.replace('_', "-"));
        assert!(
            permissions.contains(&grant.as_str()),
            "command `{cmd}` is registered but has no `{grant}` grant in \
             capabilities/default.json — it would be silently denied at runtime"
        );
    }
}

/// Strengthens `every_registered_command_has_an_acl_grant`'s one-directional
/// check (every EXPECTED command has a grant) into a bidirectional one: the
/// capability file's own command grants must equal `EXPECTED_COMMANDS`
/// exactly. The one-directional check alone would still pass if a command
/// were added to `invoke_handler!` (and even granted an ACL permission) but
/// never added to `EXPECTED_COMMANDS` itself -- it just wouldn't be checked
/// by either loop. Tauri gives no way to enumerate a `MockRuntime`'s
/// registered commands directly (`invoke_handler()` is an opaque `Fn(Invoke)
/// -> bool`), so this pins the *ACL grant set* both ways instead, per the
/// module doc's registered/ACL pairing.
#[test]
fn acl_command_grants_match_expected_commands_exactly() {
    let capabilities = include_str!("../capabilities/default.json");
    let parsed: serde_json::Value =
        serde_json::from_str(capabilities).expect("capabilities/default.json must be valid JSON");
    let permissions: Vec<&str> = parsed["permissions"]
        .as_array()
        .expect("permissions must be an array")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();

    // Our own commands' grants are the bare `allow-<command>` permissions
    // (no `plugin:` prefix) -- distinct from a plugin's own default/grant
    // like `core:default` or `global-shortcut:allow-register`, which never
    // start with the literal `allow-` prefix.
    let mut command_grants: Vec<String> = permissions
        .iter()
        .filter_map(|p| p.strip_prefix("allow-"))
        .map(|suffix| suffix.replace('-', "_"))
        .collect();
    command_grants.sort();

    let mut expected: Vec<String> = EXPECTED_COMMANDS.iter().map(|c| c.to_string()).collect();
    expected.sort();

    assert_eq!(
        command_grants, expected,
        "capabilities/default.json's `allow-*` command grants must equal \
         EXPECTED_COMMANDS exactly in both directions -- a command added to \
         the invoke handler (and even ACL-granted) but omitted from \
         EXPECTED_COMMANDS would otherwise go unchecked, and a stale grant \
         left behind for a removed command would too"
    );
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn an_unregistered_command_name_is_rejected_as_not_found() {
    // Sanity check for `is_unregistered_rejection` itself: a command that
    // was never registered anywhere must be rejected with "not found", so
    // the assertion above is actually discriminating and not vacuously true.
    let app = build_test_app();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock window");

    let result = invoke(&window, "definitely_not_a_real_command");
    let value = result.expect_err("an unregistered command must reject");
    assert!(is_unregistered_rejection(&value), "got: {value}");
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn tray_quit_exits_the_app_process() {
    // `tray_quit` calls `AppHandle::exit`, which under `MockRuntime` just
    // records the exit request rather than terminating the test process —
    // exercised here as a smoke test that the command runs at all.
    let app = build_test_app();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock window");

    let result = invoke(&window, "tray_quit");
    assert!(
        !matches!(&result, Err(v) if is_unregistered_rejection(v)),
        "tray_quit must be registered"
    );
}
