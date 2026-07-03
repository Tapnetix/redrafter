//! text-inject: capture and inject text via the platform accessibility
//! API (macOS Accessibility / Windows UI Automation), falling back to the
//! clipboard when that fails.
//!
//! The public entry points are [`capture()`] and [`inject()`]. Both are
//! AX-first: they try the accessibility path, verify it worked, and only
//! fall back to the clipboard (always saving and restoring the user's
//! prior clipboard contents) when AX fails or the write can't be
//! verified. See `AVOID` in the plan: refine must never churn the user's
//! clipboard unless AX genuinely isn't available.
//!
//! The real macOS implementation lives in [`macos`] behind
//! `#[cfg(target_os = "macos")]`, since it depends on `AXUIElement` and
//! command-line tools (`pbcopy`/`pbpaste`/`osascript`) that only exist on
//! macOS. The real Windows implementation lives in [`windows`] behind
//! `#[cfg(target_os = "windows")]`, since it depends on UI Automation/
//! SendInput/the Win32 clipboard. On every other platform
//! `capture()`/`inject()` return an "unsupported platform" error, and
//! this crate still compiles and its platform-agnostic orchestration
//! logic (in [`capture`]/[`inject`]) is unit-tested via a fake
//! [`PlatformOps`] in `tests/fallback_test.rs`.

mod capture;
mod inject;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

// Pure text-processing helpers for the Windows backend. Compiled on
// Windows (where `windows.rs` uses them for real) and under `cfg(test)`
// on every OS, so the UIA-selection text munging can still be unit
// tested on this Linux dev host even though `windows.rs` itself (gated to
// `cfg(target_os = "windows")`) cannot be built or run here.
#[cfg(any(target_os = "windows", test))]
mod windows_util;

pub use capture::{capture_with, CaptureSource, Captured};
pub use inject::inject_with;

use anyhow::Result;

/// The low-level platform operations that [`capture`]/[`inject`]
/// orchestrate. Implemented for real by `macos::MacosOps` on macOS;
/// faked in tests to exercise the AX-fails -> clipboard-fallback paths
/// without touching the OS at all.
pub trait PlatformOps {
    /// Read the currently selected text via the Accessibility API.
    ///
    /// Returns `Err` if AX access is unavailable/denied or nothing is
    /// focused. Returns `Ok(text)` otherwise — `text` may be empty if
    /// AX is reachable but there's no active selection.
    fn ax_read_selection(&self) -> Result<String>;

    /// Write `text` as the selected text of the focused element via the
    /// Accessibility API.
    ///
    /// Returns `Err` if the write could not be performed (no focused
    /// editable element, AX denied, unsupported control, etc).
    fn ax_write_selection(&self, text: &str) -> Result<()>;

    /// Read the current clipboard contents, if any.
    fn clipboard_get(&self) -> Result<Option<String>>;

    /// Overwrite the clipboard contents.
    fn clipboard_set(&self, text: &str) -> Result<()>;

    /// Simulate a "Copy" keystroke (Cmd+C) so the current selection lands
    /// on the clipboard.
    fn simulate_copy(&self) -> Result<()>;

    /// Simulate a "Paste" keystroke (Cmd+V) so the clipboard contents are
    /// injected at the current cursor/selection.
    fn simulate_paste(&self) -> Result<()>;
}

/// Capture the current text selection: AX-first, clipboard fallback.
#[cfg(target_os = "macos")]
pub fn capture() -> Result<Captured> {
    let ops = macos::MacosOps::new();
    capture::capture_with(&ops)
}

/// Capture the current text selection on Windows: UI Automation
/// TextPattern first, clipboard fallback (see [`windows`] module docs).
#[cfg(target_os = "windows")]
pub fn capture() -> Result<Captured> {
    let ops = windows::WindowsOps::new();
    capture::capture_with(&ops)
}

/// Capture is only implemented on macOS and Windows; other platforms
/// report the selection as unavailable rather than silently returning
/// nothing.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn capture() -> Result<Captured> {
    Err(anyhow::anyhow!(
        "Platform unsupported: text-inject capture is only available on macOS and Windows"
    ))
}

/// Inject `text` at the current cursor/selection: AX-first with a
/// post-write verify read, clipboard save/write/paste/restore fallback.
#[cfg(target_os = "macos")]
pub fn inject(text: &str) -> Result<()> {
    let ops = macos::MacosOps::new();
    inject::inject_with(&ops, text)
}

/// Inject `text` on Windows: UIA has no reliable selection-write, so this
/// always takes the clipboard save/write/paste (`SendInput` Ctrl+V)/
/// verify/restore fallback (see [`windows`] module docs).
#[cfg(target_os = "windows")]
pub fn inject(text: &str) -> Result<()> {
    let ops = windows::WindowsOps::new();
    inject::inject_with(&ops, text)
}

/// Inject is only implemented on macOS and Windows; other platforms
/// report the injection target as unavailable rather than silently doing
/// nothing.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn inject(_text: &str) -> Result<()> {
    Err(anyhow::anyhow!(
        "Platform unsupported: text-inject inject is only available on macOS and Windows"
    ))
}

#[cfg(all(test, not(any(target_os = "macos", target_os = "windows"))))]
mod non_macos_tests {
    use super::*;

    #[test]
    fn capture_reports_unsupported_platform() {
        let err = capture().unwrap_err();
        assert!(err.to_string().contains("Platform unsupported"));
    }

    #[test]
    fn inject_reports_unsupported_platform() {
        let err = inject("anything").unwrap_err();
        assert!(err.to_string().contains("Platform unsupported"));
    }
}
