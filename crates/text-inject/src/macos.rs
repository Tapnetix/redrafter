//! Real macOS backend for [`crate::PlatformOps`].
//!
//! AX reads/writes use `AXUIElement`/`AXAttribute` from the
//! `accessibility` crate against the system-wide focused UI element's
//! `AXSelectedText` attribute. The clipboard fallback shells out to
//! `pbcopy`/`pbpaste` (kept dependency free of Tauri/objc — plain
//! `std::process::Command`) and drives Cmd+C/Cmd+V as `CGEvent`s with
//! explicitly-set modifier flags.
//!
//! Those keystrokes used to go through `osascript`/"System Events"
//! `keystroke`, which merges the *physically held* modifiers into whatever it
//! posts — so the Cmd+C we asked for arrived as Cmd+Shift+C whenever the user
//! still had Shift down from selecting the text (or from a Shift-bearing
//! hotkey). In Slack that is the inline-code shortcut, so redrafter silently
//! reformatted the selection as code instead of copying it. See
//! `macos_util.rs` for the full write-up; the fix is to build the events
//! ourselves and call `CGEventSetFlags` with exactly Command, plus a bounded
//! wait for the user to let go of the hotkey first.
//!
//! NOTE: this module only compiles on macOS (`#[cfg(target_os =
//! "macos")]` in `lib.rs`) and cannot be built or exercised on this
//! Linux dev host. It has NOT been compiled or run for real; real-surface
//! verification (does `AXSelectedText` actually round-trip in Safari/
//! Notes/etc, does the exact `accessibility` 0.1.6 API line up with what's
//! written here) is deferred to a macOS machine. Treat this as a
//! faithful best-effort port until that verification pass happens.

use crate::macos_util::{modifiers_are_clear, KEY_CODE_C, KEY_CODE_V};
use crate::PlatformOps;
use accessibility::{AXAttribute, AXUIElement};
use anyhow::{anyhow, Context, Result};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long to wait for the user to release the hotkey's modifiers before
/// synthesizing a shortcut anyway. Setting the flags on our own events covers
/// apps that read the modifiers off the event; this additionally covers apps
/// that consult the live hardware state (`[NSEvent modifierFlags]`). Bounded
/// so a user who holds the hotkey down can never stall a refine indefinitely.
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_millis(400);

/// Poll interval while waiting for [`MODIFIER_RELEASE_TIMEOUT`].
const MODIFIER_POLL_INTERVAL: Duration = Duration::from_millis(10);

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
        run_command_keystroke(KEY_CODE_C)
    }

    fn simulate_paste(&self) -> Result<()> {
        run_command_keystroke(KEY_CODE_V)
    }
}

extern "C" {
    /// `CGEventSourceFlagsState` — the modifier flags currently in effect for
    /// a given event source. Declared here because `core-graphics` 0.23 binds
    /// `CGEventSource` but not this function; the CoreGraphics framework is
    /// already linked via that crate's default `link` feature.
    fn CGEventSourceFlagsState(state_id: CGEventSourceStateID) -> u64;
}

/// The modifier flags physically in effect right now, across the session's
/// combined hardware + synthetic state.
fn held_modifier_flags() -> u64 {
    // SAFETY: `CGEventSourceFlagsState` takes a `CGEventSourceStateID` by
    // value and returns a bitmask; no pointers or lifetimes are involved.
    unsafe { CGEventSourceFlagsState(CGEventSourceStateID::CombinedSessionState) }
}

/// Waits (up to [`MODIFIER_RELEASE_TIMEOUT`]) for every shortcut-altering
/// modifier to be released. Returns whether they actually cleared — the caller
/// proceeds either way, since the explicit flags below are the primary defence.
fn wait_for_modifiers_release() -> bool {
    let deadline = Instant::now() + MODIFIER_RELEASE_TIMEOUT;
    loop {
        if modifiers_are_clear(held_modifier_flags()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(MODIFIER_POLL_INTERVAL);
    }
}

/// Synthesizes Cmd+`keycode` as a CGEvent key-down/key-up pair with the
/// modifier flags set to *exactly* Command.
///
/// The explicit `set_flags` is the fix for the Slack "my selection turned into
/// code" report: the previous `osascript`/System Events `keystroke` inherited
/// whatever the user was still holding (Shift from selecting the text, or the
/// hotkey's own Ctrl/Alt), so Cmd+C could reach the app as Cmd+Shift+C — which
/// is Slack's inline-code binding rather than Copy. Building the event
/// ourselves means the receiving app sees the flags we chose, not the keyboard's
/// live state. Using a layout-independent key code rather than the character
/// `"c"` also fixes non-US layouts, where the key producing "c" isn't
/// necessarily the one Cmd+C is bound to.
fn run_command_keystroke(keycode: CGKeyCode) -> Result<()> {
    wait_for_modifiers_release();

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("failed to create a CGEventSource for key synthesis"))?;

    for keydown in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), keycode, keydown)
            .map_err(|_| anyhow!("failed to create a synthetic key event"))?;
        // Overrides the flags the event would otherwise carry, including any
        // live hardware modifier state.
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }

    // Give the target app a moment to react to the keystroke before we
    // read the clipboard or restore it.
    thread::sleep(Duration::from_millis(100));
    Ok(())
}
