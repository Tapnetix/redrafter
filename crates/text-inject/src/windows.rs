//! Real Windows backend for [`crate::PlatformOps`].
//!
//! Selection reads go through UI Automation (UIA): `IUIAutomation::
//! GetFocusedElement` -> `IUIAutomationTextPattern::GetSelection`,
//! joining/normalizing the resulting text range(s) via
//! [`crate::windows_util`]. This is the Windows analog of macOS's
//! `AXSelectedText` read. When the focused element has no `TextPattern`
//! (or nothing is focused, or UIA itself is unreachable), `ax_read_selection`
//! returns `Err` and the generic `capture_with` orchestration in
//! `capture.rs` falls back to the clipboard-copy path (`simulate_copy` via
//! `SendInput` Ctrl+C, then read the clipboard) -- matching the design's
//! AX-first + clipboard-fallback strategy.
//!
//! Writing is different from macOS: UI Automation has no reliable,
//! universally-supported "replace just the current selection" operation
//! analogous to macOS's settable `AXSelectedText` attribute (the closest
//! thing, `ValuePattern.SetValue`, replaces a control's *entire* value
//! and most edit controls don't implement it anyway). `ax_write_selection`
//! therefore always reports unsupported, so `inject_with` (in `inject.rs`)
//! always takes the clipboard save/write/paste(`SendInput` Ctrl+V)/verify/
//! restore fallback path, per this task's guidance.
//!
//! Clipboard get/set use the `clipboard-win` crate (`CF_UNICODETEXT`)
//! rather than hand-rolled `OpenClipboard`/`GetClipboardData` calls, to
//! cut down on raw Win32 clipboard surface that can't be checked here.
//!
//! NOTE: this module only compiles on Windows (`#[cfg(target_os =
//! "windows")]` in `lib.rs`) and cannot be built or exercised on this
//! Linux dev host -- there is no Windows toolchain or machine available
//! in this environment. It has NOT been compiled or run for real; every
//! API shape below (the exact `windows`-crate 0.61 UIA/SendInput
//! signatures, `clipboard-win` 5.x's `get`/`set`, HRESULT handling) was
//! cross-checked against that crate's published docs rather than a local
//! compiler, but real-surface verification (does `TextPattern::
//! GetSelection` actually round-trip in Notepad/Chrome/Word, does
//! `SendInput` Ctrl+C/Ctrl+V land reliably, does the exact COM lifecycle
//! here hold up) is deferred to a Windows machine. Treat this as a
//! faithful best-effort port until that verification pass happens.

use crate::windows_util;
use crate::PlatformOps;
use anyhow::{anyhow, bail, Result};
use std::cell::Cell;
use std::thread;
use std::time::Duration;
use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C,
    VK_CONTROL, VK_V,
};

pub struct WindowsOps;

impl WindowsOps {
    pub fn new() -> Self {
        Self
    }

    /// The system's `IUIAutomation` instance, created fresh per call (UIA
    /// clients are meant to be short-lived/cheap to create; we don't hold
    /// one across calls since `WindowsOps` -- like `macos::MacosOps` --
    /// carries no state between `capture()`/`inject()` invocations).
    fn automation() -> Result<IUIAutomation> {
        ensure_com_initialized();
        // SAFETY: FFI call into COM per the `windows` crate's documented
        // `CoCreateInstance` signature; `CUIAutomation` is the UIA client
        // class GUID and `IUIAutomation` the interface we request.
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| anyhow!("CoCreateInstance(CUIAutomation) failed: {e}"))
    }
}

impl Default for WindowsOps {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformOps for WindowsOps {
    fn ax_read_selection(&self) -> Result<String> {
        let automation = Self::automation()?;

        // SAFETY: `GetFocusedElement` per the crate's documented
        // `IUIAutomation` signature.
        let element = unsafe { automation.GetFocusedElement() }
            .map_err(|e| anyhow!("IUIAutomation::GetFocusedElement failed: {e}"))?;
        if Interface::as_raw(&element).is_null() {
            bail!("no focused UI Automation element");
        }

        // SAFETY: `GetCurrentPatternAs` per the crate's documented
        // `IUIAutomationElement` signature; requesting the TextPattern
        // COM interface for the pattern id `UIA_TextPatternId`.
        let text_pattern: IUIAutomationTextPattern =
            unsafe { element.GetCurrentPatternAs(UIA_TextPatternId) }.map_err(|e| {
                anyhow!(
                "focused element does not support TextPattern (GetCurrentPatternAs failed): {e}"
            )
            })?;

        // SAFETY: `GetSelection` per the crate's documented
        // `IUIAutomationTextPattern` signature.
        let selection = unsafe { text_pattern.GetSelection() }
            .map_err(|e| anyhow!("TextPattern::GetSelection failed: {e}"))?;

        // SAFETY: `Length` per the crate's documented
        // `IUIAutomationTextRangeArray` signature.
        let len = unsafe { selection.Length() }
            .map_err(|e| anyhow!("TextRangeArray::Length failed: {e}"))?;

        let mut ranges = Vec::with_capacity(len.max(0) as usize);
        for i in 0..len {
            // SAFETY: `GetElement`/`GetText` per the crate's documented
            // `IUIAutomationTextRangeArray`/`IUIAutomationTextRange`
            // signatures. `-1` for `GetText`'s `maxlength` requests the
            // full range text, matching the pattern's documented "no
            // limit" convention.
            let range = unsafe { selection.GetElement(i) }
                .map_err(|e| anyhow!("TextRangeArray::GetElement({i}) failed: {e}"))?;
            let text = unsafe { range.GetText(-1) }
                .map_err(|e| anyhow!("TextRange::GetText failed: {e}"))?;
            ranges.push(text.to_string());
        }

        Ok(windows_util::selection_text_from_ranges(&ranges))
    }

    fn ax_write_selection(&self, _text: &str) -> Result<()> {
        // UI Automation has no generic, reliably-supported "replace the
        // selection" write (see module doc). Always report unsupported so
        // `inject_with` takes the clipboard + SendInput Ctrl+V fallback,
        // which every editable control supports.
        Err(anyhow!(
            "UIA has no generic selection-write pattern; use the clipboard fallback"
        ))
    }

    fn clipboard_get(&self) -> Result<Option<String>> {
        // Mirror macOS's forgiving `pbpaste`-fails-means-empty behavior:
        // an empty/inaccessible clipboard (no `CF_UNICODETEXT` data, or a
        // transient `OpenClipboard` failure because another app has it
        // open) is reported as "nothing saved", not a hard error --
        // callers only use this to snapshot-and-restore the user's prior
        // clipboard, where "there wasn't anything" is a normal outcome.
        match clipboard_win::get::<String, _>(clipboard_win::formats::Unicode) {
            Ok(text) => Ok(Some(text)),
            Err(_) => Ok(None),
        }
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        clipboard_win::set(clipboard_win::formats::Unicode, text).map_err(|e| {
            anyhow!("SetClipboardData(CF_UNICODETEXT) via clipboard-win failed: {e:?}")
        })?;
        // Give the clipboard a moment to settle before anything reads it,
        // mirroring macOS's pbcopy settle delay.
        thread::sleep(Duration::from_millis(50));
        Ok(())
    }

    fn simulate_copy(&self) -> Result<()> {
        send_ctrl_combo(VK_C)
    }

    fn simulate_paste(&self) -> Result<()> {
        send_ctrl_combo(VK_V)
    }
}

/// Initialize COM (apartment-threaded, the model UIA clients expect) on
/// the calling thread, at most once per thread. COM apartments are
/// per-thread, not per-process, so this must run on every OS thread that
/// ends up calling `automation()` -- a process-wide "run once" guard
/// would leave every other thread uninitialized and `CoCreateInstance`
/// failing with `CO_E_NOTINITIALIZED`. We deliberately never call
/// `CoUninitialize`: text-inject makes short, one-off capture/inject
/// calls rather than owning a message loop, so "init once per thread and
/// leak it for the process's lifetime" is the standard pattern (as
/// opposed to precisely balancing init/uninit around every call).
fn ensure_com_initialized() {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
    }
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            // SAFETY: `CoInitializeEx` per the crate's documented
            // signature; `None`/no reserved pointer and apartment-
            // threaded model, called once per thread via the
            // `thread_local` guard above. We deliberately ignore the
            // result (including `RPC_E_CHANGED_MODE`, which happens if
            // this thread's COM apartment was already initialized with a
            // different threading model by other code, e.g. the host
            // app's UI thread) -- if it failed, the following
            // `CoCreateInstance` call will fail too and that failure
            // surfaces as an ordinary `Err` from `ax_read_selection`,
            // which `capture_with`'s existing clipboard fallback already
            // handles.
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            initialized.set(true);
        }
    });
}

/// Simulate Ctrl+`key` via `SendInput`. SendKeys-style APIs are unreliable
/// across applications; `SendInput` is the recommended low-level substitute.
fn send_ctrl_combo(key: VIRTUAL_KEY) -> Result<()> {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(key, false),
        key_input(key, true),
        key_input(VK_CONTROL, true),
    ];

    // SAFETY: `SendInput` per the crate's documented signature; `inputs`
    // is a valid, fully-initialized `&[INPUT]` and `cbsize` is
    // `size_of::<INPUT>()` as required.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        bail!(
            "SendInput only delivered {sent} of {} key events",
            inputs.len()
        );
    }
    // Give the target app a moment to react to the keystrokes before we
    // read/restore the clipboard, mirroring macOS's post-keystroke delay.
    thread::sleep(Duration::from_millis(100));
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
