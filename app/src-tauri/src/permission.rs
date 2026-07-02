// Accessibility permission status.
//
// Text capture/inject (`text-inject`) needs the macOS Accessibility
// permission to read/write the focused element via the AX API. This module
// reports whether it is currently granted and can open the System Settings
// pane so the user can grant it.
//
// The real check (`AXIsProcessTrusted`) is macOS-only and lives behind
// `#[cfg(target_os = "macos")]`; every other platform has no such gate and
// reports granted, so this module builds and tests cleanly on Linux/CI. The
// macOS behavior itself is verified on a real Mac, not here.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/core_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block.)

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PermissionStatus {
    pub granted: bool,
}

/// Seam over the OS-level trust check so the status mapping can be
/// unit-tested without touching real platform APIs.
pub trait AccessibilityChecker {
    fn is_trusted(&self) -> bool;
}

/// The real, platform-backed checker.
pub struct SystemAccessibilityChecker;

impl AccessibilityChecker for SystemAccessibilityChecker {
    fn is_trusted(&self) -> bool {
        platform_is_trusted()
    }
}

/// Maps a checker's trust bit onto the `PermissionStatus` the frontend
/// consumes. Pulled out of `permission_status` so it is testable with a fake
/// `AccessibilityChecker` on any platform.
pub fn status_from<C: AccessibilityChecker>(checker: &C) -> PermissionStatus {
    PermissionStatus {
        granted: checker.is_trusted(),
    }
}

#[cfg(target_os = "macos")]
mod ffi {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        /// Returns true when the current process is trusted for
        /// Accessibility. Does not prompt the user.
        pub fn AXIsProcessTrusted() -> bool;
    }
}

#[cfg(target_os = "macos")]
fn platform_is_trusted() -> bool {
    unsafe { ffi::AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
fn platform_is_trusted() -> bool {
    // Accessibility is a macOS-specific gate; other platforms have nothing
    // to grant, so capture/inject is never blocked here.
    true
}

/// Tauri command: reports whether the Accessibility permission is granted.
#[tauri::command]
pub fn permission_status() -> PermissionStatus {
    status_from(&SystemAccessibilityChecker)
}

/// Tauri command: opens the Accessibility pane in System Settings.
#[tauri::command]
pub fn permission_open_settings() -> Result<(), String> {
    open_settings_platform()
}

#[cfg(target_os = "macos")]
fn open_settings_platform() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
fn open_settings_platform() -> Result<(), String> {
    // No Accessibility pane to open outside macOS.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeChecker(bool);

    impl AccessibilityChecker for FakeChecker {
        fn is_trusted(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn status_from_maps_granted_true() {
        assert_eq!(
            status_from(&FakeChecker(true)),
            PermissionStatus { granted: true }
        );
    }

    #[test]
    fn status_from_maps_granted_false() {
        assert_eq!(
            status_from(&FakeChecker(false)),
            PermissionStatus { granted: false }
        );
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn permission_status_is_always_granted_on_non_macos() {
        assert_eq!(permission_status(), PermissionStatus { granted: true });
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn permission_open_settings_is_a_noop_on_non_macos() {
        assert_eq!(permission_open_settings(), Ok(()));
    }
}
