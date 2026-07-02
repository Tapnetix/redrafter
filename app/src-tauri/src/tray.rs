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

        tray.on_menu_event(move |app, event| match event.id().as_ref() {
            "refine" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::run_refine(&handle).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });
    } else {
        eprintln!("[tray] tray icon 'main' not found — tray menu not configured");
    }

    Ok(())
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
