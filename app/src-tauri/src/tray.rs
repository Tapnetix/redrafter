// Menu-bar tray skeleton (wireframes/tray.html, controls/tray.json).
//
// Phase A only needs a status icon plus the two menu entries this task's
// commands back: Refine (`tray_refine`, honoring the same permission/
// active-model gating as the frontend's `refine` command) and Quit
// (`tray_quit`). The full state-reflecting switcher/favorites/pause/updates
// menu (S? of tray.html) is Phase B/C's job and extends `build_menu` /
// `on_menu_event` below rather than forking a new module.
//
// The tray icon itself (id "main") is created by Tauri from
// `tauri.conf.json`'s `app.trayIcon` block; this module only attaches the
// menu/tooltip/event handlers to it once the app is running.

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    Runtime,
};

/// Attaches the Phase A tray menu (Refine, Quit) and a ready tooltip to the
/// tray icon declared in `tauri.conf.json`. Logs (rather than errors out)
/// when the icon isn't found so a headless/CI build without a real tray
/// backend doesn't fail app setup.
///
/// Not unit-tested directly: on Linux, building a native menu (even without
/// a tray icon attached to it) needs a running GTK main loop, which only
/// exists once the real app has called `Builder::run` — a plain `#[test]`
/// under `tauri::test::MockRuntime` panics with "GTK has not been
/// initialized" before this function's first line even returns. This is the
/// "real tray icon attach" glue the coverage gate allows as an exception;
/// [`handle_menu_event`] below (the actual branch logic this function wires
/// up) is extracted specifically so it *is* unit-testable.
pub fn setup_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let refine = MenuItemBuilder::with_id("refine", "Refine selection").build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit redrafter").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&refine)
        .item(&separator)
        .item(&quit)
        .build()?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
        tray.set_tooltip(Some("redrafter — Ready"))?;
        tray.on_menu_event(move |app, event| handle_menu_event(app, event.id().as_ref()));
    } else {
        eprintln!("[tray] tray icon 'main' not found — tray menu not configured");
    }

    Ok(())
}

/// Reacts to a tray menu selection: triggers a refine (best-effort, same
/// pipeline as the `refine`/`tray_refine` commands) for "refine", or exits
/// the app for "quit". Anything else is ignored.
///
/// Pulled out of `setup_tray`'s `on_menu_event` closure so this branch logic
/// is unit-testable directly: a real tray icon (and so a real menu event)
/// can't be created under `tauri::test::MockRuntime` (its `tray_icon` is
/// always `None`), so `setup_tray` itself can only ever exercise its "no
/// tray icon" fallback in tests — this function is what covers the actual
/// dispatch logic.
fn handle_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    match id {
        "refine" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::run_refine(&handle).await;
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Tauri command: triggers the same default refine pipeline as the
/// frontend's `refine` command, from the tray's Refine entry. Registered
/// by A14 (`app/src-tauri/src/lib.rs`).
#[tauri::command]
pub async fn tray_refine<R: Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    crate::run_refine(&app).await.map(|_| ())
}

/// Tauri command: quits the app from the tray's Quit entry (and the
/// Capture panel's display-only tray preview, per its carve-out).
/// Registered by A14 (`app/src-tauri/src/lib.rs`).
#[tauri::command]
pub fn tray_quit<R: Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
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
    /// `setup_tray` is built to handle.
    fn managed_app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::generate_context!())
            .expect("failed to build mock app");
        app.manage(SettingsStore::open_in_memory().expect("failed to open in-memory settings"));
        app.manage(ConnectionStore::open_in_memory().expect("failed to open in-memory connections"));
        app.manage(RefineState::default());
        app
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
        handle_menu_event(&app.handle().clone(), "refine");
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
}
