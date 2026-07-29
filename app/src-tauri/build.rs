// Lists every custom command so `tauri_build` autogenerates the
// `allow-<command>`/`deny-<command>` ACL permissions `capabilities/default.json`
// grants — without this, Tauri v2's runtime ACL denies every custom command
// by default (any invoke rejects with "not allowed"), which would silently
// break the whole hotkey -> refine -> inject -> restore loop.
//
// This list must stay in lockstep with `lib.rs`'s `invoke_handler!` and
// `capabilities/default.json`'s `permissions` — the wire-up gate
// (`tests/wireup_test.rs`) asserts every registered command is actually
// granted, since a registered-but-ungranted command is silently denied at
// runtime. B23 adds the Phase B commands below A14's Phase A set; C17 adds
// the Phase C commands (presets/history/feedback) below that.
const COMMANDS: &[&str] = &[
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

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri_build");
}
