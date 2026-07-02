// Lists every Phase A command so `tauri_build` autogenerates the
// `allow-<command>`/`deny-<command>` ACL permissions `capabilities/default.json`
// grants — without this, Tauri v2's runtime ACL denies every custom command
// by default (any invoke rejects with "not allowed"), which would silently
// break the whole hotkey -> refine -> inject -> restore loop.
const COMMANDS: &[&str] = &[
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

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri_build");
}
