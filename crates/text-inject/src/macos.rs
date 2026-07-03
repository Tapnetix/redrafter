//! Real macOS backend for [`crate::PlatformOps`].
//!
//! AX reads/writes use `AXUIElement`/`AXAttribute` from the
//! `accessibility` crate against the system-wide focused UI element's
//! `AXSelectedText` attribute. The clipboard fallback shells out to
//! `pbcopy`/`pbpaste` and drives Cmd+C/Cmd+V via `osascript` "System
//! Events" keystrokes, mirroring a sibling project's
//! `src-tauri/src/platform/macos.rs` clipboard handling (kept dependency
//! free of Tauri/objc — plain `std::process::Command`).
//!
//! NOTE: this module only compiles on macOS (`#[cfg(target_os =
//! "macos")]` in `lib.rs`) and cannot be built or exercised on this
//! Linux dev host. It has NOT been compiled or run for real; real-surface
//! verification (does `AXSelectedText` actually round-trip in Safari/
//! Notes/etc, does the exact `accessibility` 0.1.6 API line up with what's
//! written here) is deferred to a macOS machine. Treat this as a
//! faithful best-effort port until that verification pass happens.

use crate::PlatformOps;
use accessibility::{AXAttribute, AXUIElement};
use anyhow::{anyhow, Context, Result};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub struct MacosOps;

impl MacosOps {
    pub fn new() -> Self {
        Self
    }

    /// A generic (untyped) `AXAttribute<CFType>` for `name`. The
    /// `accessibility` 0.1.x crate only predefines a handful of typed
    /// attributes (via its `define_attributes!` macro) and neither
    /// `AXFocusedUIElement` nor `AXSelectedText` is among them, so we build
    /// them by name and downcast the returned `CFType` ourselves.
    fn attr(name: &'static str) -> AXAttribute<CFType> {
        AXAttribute::new(&CFString::from_static_string(name))
    }

    /// The system-wide currently-focused UI element, e.g. a text field or
    /// text view in whatever app the user is in.
    fn focused_element() -> Result<AXUIElement> {
        let value = AXUIElement::system_wide()
            .attribute(&Self::attr("AXFocusedUIElement"))
            .map_err(|e| anyhow!("no focused UI element: {e:?}"))?;
        value
            .downcast_into::<AXUIElement>()
            .ok_or_else(|| anyhow!("AXFocusedUIElement was not an AXUIElement"))
    }

    fn selected_text_attribute() -> AXAttribute<CFType> {
        Self::attr("AXSelectedText")
    }
}

impl Default for MacosOps {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformOps for MacosOps {
    fn ax_read_selection(&self) -> Result<String> {
        let element = Self::focused_element()?;
        let value = element
            .attribute(&Self::selected_text_attribute())
            .map_err(|e| anyhow!("AXSelectedText read failed: {e:?}"))?;
        let text = value
            .downcast_into::<CFString>()
            .ok_or_else(|| anyhow!("AXSelectedText was not a string"))?;
        Ok(text.to_string())
    }

    fn ax_write_selection(&self, text: &str) -> Result<()> {
        let element = Self::focused_element()?;
        element
            .set_attribute(
                &Self::selected_text_attribute(),
                CFString::new(text).as_CFType(),
            )
            .map_err(|e| anyhow!("AXSelectedText write failed: {e:?}"))?;
        Ok(())
    }

    fn clipboard_get(&self) -> Result<Option<String>> {
        let output = Command::new("pbpaste")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .context("failed to run pbpaste")?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
        } else {
            Ok(None)
        }
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to run pbcopy")?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .context("failed to write to pbcopy stdin")?;
        }
        let status = child.wait().context("failed to wait on pbcopy")?;
        if !status.success() {
            anyhow::bail!("pbcopy exited with status {status}");
        }
        // Give the pasteboard a moment to settle before anything reads it.
        thread::sleep(Duration::from_millis(50));
        Ok(())
    }

    fn simulate_copy(&self) -> Result<()> {
        run_keystroke("c")
    }

    fn simulate_paste(&self) -> Result<()> {
        run_keystroke("v")
    }
}

/// Simulate Cmd+`key` via `osascript`/System Events, the same mechanism
/// a sibling project uses for keyboard injection.
fn run_keystroke(key: &str) -> Result<()> {
    let status = Command::new("osascript")
        .args([
            "-e",
            &format!(r#"tell application "System Events" to keystroke "{key}" using command down"#),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run osascript")?;
    if !status.success() {
        anyhow::bail!("osascript keystroke exited with status {status}");
    }
    // Give the target app a moment to react to the keystroke before we
    // read the clipboard or restore it.
    thread::sleep(Duration::from_millis(100));
    Ok(())
}
