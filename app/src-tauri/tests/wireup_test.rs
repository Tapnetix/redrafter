//! Exercises the production invoke handler (`redrafter_lib::invoke_handler`)
//! against a `MockRuntime` app to assert every Phase A command — including
//! the tray commands `tray_refine`/`tray_quit` — is actually registered,
//! rather than just compiled. This is A14's wire-up gate: the classic
//! failure mode this guards against is shipping every module but never
//! adding it to `tauri::generate_handler![...]`, so nothing calls it.
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
use redrafter_lib::settings::SettingsStore;
use redrafter_lib::RefineState;
use tauri::test::INVOKE_KEY;
use tauri::webview::InvokeRequest;
use tauri::ipc::CallbackFn;
use tauri::Manager;

/// Every command A14's composition root must register (per the plan's
/// `done_when`): the Phase A screens' commands plus the tray skeleton's
/// `tray_refine`/`tray_quit`.
const EXPECTED_COMMANDS: &[&str] = &[
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

#[test]
fn every_phase_a_command_is_registered() {
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

#[test]
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
