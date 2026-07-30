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
//! NOTE: this module only compiles on macOS (`#[cfg(target_os = "macos")]` in
//! `lib.rs`), so it cannot be built from a Linux dev host — use
//! `cargo clippy -p text-inject --target aarch64-apple-darwin --all-targets`
//! there, which typechecks it without linking.
//!
//! Verified on a real Mac (macOS 14.1, arm64) so far: it compiles and links;
//! `CGEventSetFlags` produces exactly the flags asked for; and the
//! `pbcopy`/`pbpaste` pair round-trips non-ASCII text correctly with
//! [`UTF8_LOCALE`] forced (and demonstrably does not without it). Still
//! unverified on real hardware: that `AXSelectedText` round-trips in a given
//! app, and that a synthesized shortcut lands with the intended modifiers in
//! the receiving app — both need Accessibility permission, which a shell
//! session doesn't have.

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

/// The locale forced on `pbcopy`/`pbpaste`.
///
/// Both pick their text encoding from the locale environment, and fall back to
/// **Mac OS Roman** when it says nothing. A `.app` launched from Finder, the
/// Dock, or a login item inherits no `LANG`/`LC_*` at all — only a
/// shell-launched process gets those — so every non-ASCII refine came back
/// mangled: the UTF-8 bytes for "Это" (`d0 ad d1 82 d0 be`) were decoded as Mac
/// OS Roman and pasted as "–≠—Ç–æ".
///
/// Worse, this was invisible to `inject_via_clipboard`'s read-back check:
/// `pbpaste` re-encodes with the same wrong locale, so the bytes round-trip
/// intact and the verify passed while the pasteboard held nonsense.
///
/// `LC_ALL` as well as `LC_CTYPE` because `LC_ALL` wins over `LC_CTYPE` when
/// set, and we want a deterministic encoding regardless of what the app
/// happened to inherit.
const UTF8_LOCALE: &str = "en_US.UTF-8";

/// Forces [`UTF8_LOCALE`] on a pasteboard subprocess.
fn utf8_locale(cmd: &mut Command) -> &mut Command {
    cmd.env("LC_ALL", UTF8_LOCALE).env("LC_CTYPE", UTF8_LOCALE)
}

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
        let output = utf8_locale(&mut Command::new("pbpaste"))
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
        let mut child = utf8_locale(&mut Command::new("pbcopy"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// The pasteboard corruption this guards against reproduces as: UTF-8 bytes
    /// decoded as Mac OS Roman, so "Это" pastes as "–≠—Ç–æ". Fast and
    /// dependency-free — it only inspects the command we would spawn, so it
    /// runs anywhere macOS builds, unlike the real-pasteboard test below.
    #[test]
    fn pasteboard_commands_force_a_utf8_locale() {
        let mut cmd = Command::new("pbcopy");
        utf8_locale(&mut cmd);

        let envs: Vec<(Option<&OsStr>, Option<&OsStr>)> = cmd
            .get_envs()
            .map(|(k, v)| (Some(k), v))
            .collect();

        for key in ["LC_ALL", "LC_CTYPE"] {
            let found = envs
                .iter()
                .find(|(k, _)| *k == Some(OsStr::new(key)))
                .unwrap_or_else(|| panic!("{key} is not set on the pasteboard command"));
            assert_eq!(
                found.1,
                Some(OsStr::new(UTF8_LOCALE)),
                "{key} must pin the pasteboard encoding to UTF-8"
            );
        }
    }

    /// Drives the REAL pasteboard. `#[ignore]`d because it needs both a live
    /// pasteboard server (absent on a headless CI agent) and an environment
    /// with no inherited locale — the condition a Finder-launched .app is in,
    /// and the only one under which the bug reproduces. Run it by hand on a
    /// Mac with:
    ///
    /// ```text
    /// env -u LANG -u LC_ALL -u LC_CTYPE cargo test -p text-inject \
    ///     --lib clipboard_round_trips -- --ignored --nocapture
    /// ```
    ///
    /// Without `utf8_locale` this fails with the pasteboard holding
    /// "–≠—Ç–æ …" while our own read-back still reports the correct text —
    /// which is exactly why `inject_via_clipboard`'s verify never caught it.
    #[test]
    #[ignore = "needs a live pasteboard and a locale-free environment; see the doc comment"]
    fn clipboard_round_trips_non_ascii_without_an_inherited_locale() {
        const RUSSIAN: &str = "Это очень простой тест перевода.";
        assert!(
            std::env::var("LANG").is_err() && std::env::var("LC_ALL").is_err(),
            "run with `env -u LANG -u LC_ALL -u LC_CTYPE`, else this proves nothing"
        );

        let ops = MacosOps::new();
        let saved = ops.clipboard_get().expect("pbpaste");
        ops.clipboard_set(RUSSIAN).expect("pbcopy");

        // Read the pasteboard independently, forcing UTF-8, so a wrong-encoding
        // write can't hide behind a matching wrong-encoding read.
        let out = Command::new("pbpaste")
            .env("LC_ALL", UTF8_LOCALE)
            .env("LC_CTYPE", UTF8_LOCALE)
            .output()
            .expect("pbpaste");
        let actual = String::from_utf8_lossy(&out.stdout).to_string();

        let round_trip = ops.clipboard_get().expect("pbpaste").unwrap_or_default();
        if let Some(prior) = saved {
            let _ = ops.clipboard_set(&prior);
        }

        assert_eq!(actual, RUSSIAN, "the pasteboard does not hold correct UTF-8");
        assert_eq!(round_trip, RUSSIAN, "our own read-back is wrong");
    }
}
