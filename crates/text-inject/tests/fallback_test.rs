//! Platform-agnostic tests for the AX-fails -> clipboard-fallback
//! orchestration in `capture.rs`/`inject.rs`. Runs on any OS since it
//! exercises `capture_with`/`inject_with` against a fake `PlatformOps`
//! that never touches the real OS.

use anyhow::{anyhow, Result};
use std::cell::RefCell;
use text_inject::{capture_with, inject_with, CaptureSource, PlatformOps};

/// Fake backend that lets tests force AX to fail (or succeed) and
/// observe exactly what happens to a simulated clipboard.
struct FakeOps {
    ax_fails: bool,
    /// What a "Cmd+C" would put on the clipboard, i.e. the real
    /// on-screen selection AX can't see because it's "failing".
    selection: RefCell<String>,
    clipboard: RefCell<Option<String>>,
    /// Every value ever written to the clipboard, in order — lets tests
    /// assert save -> write -> restore ordering.
    clipboard_writes: RefCell<Vec<String>>,
}

impl FakeOps {
    fn new(ax_fails: bool, selection: &str, initial_clipboard: Option<&str>) -> Self {
        Self {
            ax_fails,
            selection: RefCell::new(selection.to_string()),
            clipboard: RefCell::new(initial_clipboard.map(str::to_string)),
            clipboard_writes: RefCell::new(vec![]),
        }
    }
}

impl PlatformOps for FakeOps {
    fn ax_read_selection(&self) -> Result<String> {
        if self.ax_fails {
            Err(anyhow!("AX read failed"))
        } else {
            Ok(self.selection.borrow().clone())
        }
    }

    fn ax_write_selection(&self, text: &str) -> Result<()> {
        if self.ax_fails {
            Err(anyhow!("AX write failed"))
        } else {
            *self.selection.borrow_mut() = text.to_string();
            Ok(())
        }
    }

    fn clipboard_get(&self) -> Result<Option<String>> {
        Ok(self.clipboard.borrow().clone())
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        self.clipboard_writes.borrow_mut().push(text.to_string());
        *self.clipboard.borrow_mut() = Some(text.to_string());
        Ok(())
    }

    fn simulate_copy(&self) -> Result<()> {
        // Pretend Cmd+C landed the current selection on the clipboard —
        // this is how a real OS copy would behave even though AX itself
        // is unreachable.
        let selected = self.selection.borrow().clone();
        *self.clipboard.borrow_mut() = Some(selected);
        Ok(())
    }

    fn simulate_paste(&self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn capture_uses_ax_when_available_and_never_touches_clipboard() {
    let ops = FakeOps::new(false, "ax selection", Some("prior clipboard"));

    let captured = capture_with(&ops).unwrap();

    assert_eq!(captured.text, "ax selection");
    assert_eq!(captured.source, CaptureSource::Accessibility);
    assert!(
        ops.clipboard_writes.borrow().is_empty(),
        "AX success path must not touch the clipboard"
    );
}

#[test]
fn capture_falls_back_to_clipboard_when_ax_read_fails() {
    let ops = FakeOps::new(
        true,
        "selected text only visible via copy",
        Some("prior clipboard"),
    );

    let captured = capture_with(&ops).expect("capture should succeed via clipboard fallback");

    assert_eq!(captured.text, "selected text only visible via copy");
    assert_eq!(captured.source, CaptureSource::Clipboard);
    // The user's prior clipboard contents must be restored afterward.
    assert_eq!(
        ops.clipboard_get().unwrap(),
        Some("prior clipboard".to_string())
    );
}

#[test]
fn capture_fallback_restores_clipboard_even_when_it_was_empty() {
    let ops = FakeOps::new(true, "some selection", None);

    let captured = capture_with(&ops).unwrap();

    assert_eq!(captured.text, "some selection");
    // Nothing to restore, so clipboard is left holding the copied text —
    // it must NOT be force-cleared, since that would itself be an
    // unwanted clipboard mutation.
    assert_eq!(
        ops.clipboard_get().unwrap(),
        Some("some selection".to_string())
    );
}

#[test]
fn inject_uses_ax_when_available_and_never_touches_clipboard() {
    let ops = FakeOps::new(false, "old", Some("prior clipboard"));

    inject_with(&ops, "new text").unwrap();

    assert_eq!(ops.ax_read_selection().unwrap(), "new text");
    assert!(
        ops.clipboard_writes.borrow().is_empty(),
        "AX success + verify path must not touch the clipboard"
    );
}

#[test]
fn inject_falls_back_to_clipboard_saving_writing_verifying_and_restoring_when_ax_fails() {
    let ops = FakeOps::new(true, "", Some("prior clipboard"));

    inject_with(&ops, "new text").expect("inject should succeed via clipboard fallback");

    // Save -> write -> restore, in that order.
    assert_eq!(
        *ops.clipboard_writes.borrow(),
        vec!["new text".to_string(), "prior clipboard".to_string()]
    );
    // The user's prior clipboard contents must be restored afterward.
    assert_eq!(
        ops.clipboard_get().unwrap(),
        Some("prior clipboard".to_string())
    );
}

#[test]
fn inject_fallback_restores_clipboard_even_when_it_was_empty() {
    let ops = FakeOps::new(true, "", None);

    inject_with(&ops, "new text").unwrap();

    // Nothing was saved, so nothing to restore — the write to "new text"
    // is the only clipboard mutation.
    assert_eq!(*ops.clipboard_writes.borrow(), vec!["new text".to_string()]);
}
