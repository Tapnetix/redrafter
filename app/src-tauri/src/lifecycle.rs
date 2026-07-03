//! App-lifecycle backend: update checks/apply via `tauri-plugin-updater`,
//! and launch-at-login via `tauri-plugin-autostart`. `tray.rs`'s native menu
//! ("Check for updates…"/"Launch at login") is the first caller of the
//! commands here; `controls/index.json`'s General screen (`check-updates`/
//! `general-launch-login`) is expected to drive the same backend.
//!
//! RELEASE-BLOCKER: `tauri.conf.json`'s `plugins.updater.pubkey` is an
//! empty placeholder. `tauri-plugin-updater` refuses to trust a signed
//! update artifact without a real Ed25519 keypair, and generating/shipping
//! one is a release-time step this task cannot perform (see this task's
//! completion notes) -- do NOT replace the placeholder with a fake key,
//! since that would make an unsigned/unverified update appear valid. The
//! check/apply logic below is built and tested against the *plugin's* own
//! seam, so it starts working for real the moment a real keypair lands in
//! `tauri.conf.json` and the matching private key signs a release.

use tauri::{AppHandle, Runtime};

use crate::settings::SettingsStore;

// ---------------------------------------------------------------------
// Updater
// ---------------------------------------------------------------------

/// Result of an update check: whether a newer version is available, and
/// which one. Mirrors `ipc.ts`'s `CheckUpdatesResult`
/// (`#[serde(rename_all = "camelCase")]`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdatesResult {
    pub update_available: bool,
    pub version: Option<String>,
}

/// Maps the plugin's already-awaited check outcome -- `Ok(Some(version))`,
/// `Ok(None)`, or `Err(..)` -- into the wire result `tray_check_updates`
/// resolves with, propagating an error unchanged. Split from the real async
/// network call so update-available/up-to-date/error are each unit-testable
/// without a live updater endpoint.
fn check_updates_impl(
    checked: Result<Option<String>, String>,
) -> Result<CheckUpdatesResult, String> {
    checked.map(|version| match version {
        Some(version) => CheckUpdatesResult {
            update_available: true,
            version: Some(version),
        },
        None => CheckUpdatesResult {
            update_available: false,
            version: None,
        },
    })
}

/// Tauri command: triggers an application-update check from the tray
/// (`Tray.tsx`) or a settings screen, via `tauri-plugin-updater`. See
/// `check_updates_impl` for the tested update-available/up-to-date/error
/// mapping; the network call itself is untested glue (no live updater
/// endpoint in CI, and no managed updater state under `MockRuntime`).
#[tauri::command]
pub async fn tray_check_updates<R: Runtime>(
    app: AppHandle<R>,
) -> Result<CheckUpdatesResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    let checked = match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(update) => Ok(update.map(|u| u.version)),
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    };
    check_updates_impl(checked)
}

/// Seam over `tauri_plugin_updater::Update::download_and_install`, so
/// `apply_update_impl` (the "install the update the last check found"
/// trigger) is unit-testable with a fake instead of a live download. Not
/// yet wired to a command: no wireframe (`wireframes/tray.html`,
/// `wireframes/index.html`) exposes an "Install now" action -- both only
/// report availability -- so this is the tested backend seam a later task
/// wires up once one does.
#[async_trait::async_trait]
pub trait UpdateApplier {
    async fn download_and_install(&self) -> Result<(), String>;
}

/// Production `UpdateApplier`, wrapping the `Update` handle a
/// `tray_check_updates` call resolved.
pub struct RealUpdateApplier(pub tauri_plugin_updater::Update);

#[async_trait::async_trait]
impl UpdateApplier for RealUpdateApplier {
    async fn download_and_install(&self) -> Result<(), String> {
        self.0
            .download_and_install(|_chunk_len, _content_len| {}, || {})
            .await
            .map_err(|e| e.to_string())
    }
}

/// Downloads and installs a pending update through `applier`. Extracted so
/// the apply trigger is unit-testable (success/failure) independent of a
/// live download -- see `RealUpdateApplier`/`UpdateApplier`. `pub` (rather
/// than `pub(crate)`) since it's ready for a later task to wire to an
/// "Install now" command the moment a wireframe adds that affordance (see
/// this function's module doc note).
pub async fn apply_update_impl<A: UpdateApplier>(applier: &A) -> Result<(), String> {
    applier.download_and_install().await
}

// ---------------------------------------------------------------------
// Launch at login
// ---------------------------------------------------------------------

/// Settings key the launch-at-login preference persists under. Unset
/// defaults to `true` (on by default, opt out) -- matches `Tray.tsx`'s own
/// default framing for a capture-utility app.
pub(crate) const LAUNCH_AT_LOGIN_SETTING_KEY: &str = "launch_at_login";

/// Reads the persisted launch-at-login preference (default: enabled).
/// Shared by `tray.rs`'s menu build and (once a General settings screen
/// wires it) that screen, so both reflect the same persisted choice.
pub(crate) fn launch_at_login_enabled(settings: &SettingsStore) -> bool {
    settings
        .get(LAUNCH_AT_LOGIN_SETTING_KEY)
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Seam over the real `tauri-plugin-autostart` call, so
/// `set_launch_at_login_impl` is unit-testable with a fake, independent of
/// the plugin's managed state (unavailable under `tauri::test::MockRuntime`).
trait LaunchAtLoginController {
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
}

/// Production `LaunchAtLoginController`, backed by
/// `tauri_plugin_autostart`'s real `AutoLaunchManager`.
struct RealLaunchAtLogin<R: Runtime>(AppHandle<R>);

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
/// Generic over `LaunchAtLoginController` (mirrors `permission_gate`'s
/// `AccessibilityChecker` pattern in `lib.rs`) so this is unit-testable with
/// a fake instead of the real OS-level autostart mechanism.
fn set_launch_at_login_impl<C: LaunchAtLoginController>(
    controller: &C,
    settings: &SettingsStore,
    enabled: bool,
) -> Result<(), String> {
    controller.set_enabled(enabled)?;
    settings
        .set(
            LAUNCH_AT_LOGIN_SETTING_KEY,
            if enabled { "true" } else { "false" },
        )
        .map_err(|e| e.to_string())
}

/// Tauri command: toggles whether redrafter launches automatically at
/// login (`Tray.tsx`'s "Launch at login"), persisting the preference and
/// driving the real `tauri-plugin-autostart` (see `RealLaunchAtLogin`).
/// Also refreshes the native tray menu so a reopened dropdown reflects the
/// new choice.
#[tauri::command]
pub fn tray_set_launch_login<R: Runtime>(
    app: AppHandle<R>,
    settings: tauri::State<'_, SettingsStore>,
    enabled: bool,
) -> Result<(), String> {
    let controller = RealLaunchAtLogin(app.clone());
    set_launch_at_login_impl(&controller, &settings, enabled)?;
    crate::tray::refresh_tray(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- check_updates_impl ----

    #[test]
    fn check_updates_impl_reports_up_to_date_for_none() {
        assert_eq!(
            check_updates_impl(Ok(None)),
            Ok(CheckUpdatesResult {
                update_available: false,
                version: None,
            })
        );
    }

    #[test]
    fn check_updates_impl_reports_the_available_version() {
        assert_eq!(
            check_updates_impl(Ok(Some("1.2.3".to_string()))),
            Ok(CheckUpdatesResult {
                update_available: true,
                version: Some("1.2.3".to_string()),
            })
        );
    }

    #[test]
    fn check_updates_impl_propagates_a_check_error() {
        let err = check_updates_impl(Err("network unreachable".to_string())).unwrap_err();
        assert_eq!(err, "network unreachable");
    }

    // ---- apply_update_impl (fake applier) ----

    struct FakeApplier {
        fail: bool,
    }

    #[async_trait::async_trait]
    impl UpdateApplier for FakeApplier {
        async fn download_and_install(&self) -> Result<(), String> {
            if self.fail {
                Err("simulated download failure".to_string())
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn apply_update_impl_succeeds_via_the_applier() {
        let applier = FakeApplier { fail: false };
        apply_update_impl(&applier).await.unwrap();
    }

    #[tokio::test]
    async fn apply_update_impl_propagates_the_applier_error() {
        let applier = FakeApplier { fail: true };
        let err = apply_update_impl(&applier).await.unwrap_err();
        assert!(err.contains("simulated"), "got: {err}");
    }

    // ---- launch_at_login_enabled ----

    #[test]
    fn launch_at_login_enabled_defaults_to_true_when_unset() {
        let settings = SettingsStore::open_in_memory().unwrap();
        assert!(launch_at_login_enabled(&settings));
    }

    #[test]
    fn launch_at_login_enabled_reflects_a_persisted_false() {
        let settings = SettingsStore::open_in_memory().unwrap();
        settings.set(LAUNCH_AT_LOGIN_SETTING_KEY, "false").unwrap();
        assert!(!launch_at_login_enabled(&settings));
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
