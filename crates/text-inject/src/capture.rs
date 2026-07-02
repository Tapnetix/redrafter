use crate::PlatformOps;
use anyhow::Result;

/// Where the captured text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSource {
    /// Read directly via the Accessibility API.
    Accessibility,
    /// AX read failed or returned nothing; fell back to the clipboard.
    Clipboard,
}

/// The result of a [`capture_with`] (or [`crate::capture`]) call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: String,
    pub source: CaptureSource,
}

/// AX-first, clipboard-fallback capture, generic over [`PlatformOps`] so
/// it can be unit tested without touching the real OS.
///
/// AX is tried first since it's silent (no clipboard churn). If AX is
/// unreachable, denied, or reports no selection, we fall back to a
/// copy-via-clipboard: save whatever's currently on the clipboard,
/// simulate a Cmd+C, read the result, then restore the user's prior
/// clipboard contents so refine never clobbers their clipboard history.
pub fn capture_with<P: PlatformOps>(ops: &P) -> Result<Captured> {
    if let Ok(text) = ops.ax_read_selection() {
        if !text.is_empty() {
            return Ok(Captured {
                text,
                source: CaptureSource::Accessibility,
            });
        }
    }

    capture_via_clipboard(ops)
}

fn capture_via_clipboard<P: PlatformOps>(ops: &P) -> Result<Captured> {
    let saved = ops.clipboard_get().unwrap_or(None);

    ops.simulate_copy()?;
    let text = ops.clipboard_get()?.unwrap_or_default();

    if let Some(prior) = saved {
        ops.clipboard_set(&prior)?;
    }

    Ok(Captured {
        text,
        source: CaptureSource::Clipboard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::cell::RefCell;

    /// Minimal in-module fake, separate from `tests/fallback_test.rs`'s
    /// fuller fake, just to pin down the empty-selection edge case at the
    /// unit level.
    struct EmptySelectionOps {
        clipboard: RefCell<Option<String>>,
    }

    impl PlatformOps for EmptySelectionOps {
        fn ax_read_selection(&self) -> Result<String> {
            // AX reachable, but nothing is selected.
            Ok(String::new())
        }
        fn ax_write_selection(&self, _text: &str) -> Result<()> {
            Err(anyhow!("not used in this test"))
        }
        fn clipboard_get(&self) -> Result<Option<String>> {
            Ok(self.clipboard.borrow().clone())
        }
        fn clipboard_set(&self, text: &str) -> Result<()> {
            *self.clipboard.borrow_mut() = Some(text.to_string());
            Ok(())
        }
        fn simulate_copy(&self) -> Result<()> {
            *self.clipboard.borrow_mut() = Some("copied via cmd+c".to_string());
            Ok(())
        }
        fn simulate_paste(&self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn empty_ax_selection_falls_back_to_clipboard() {
        let ops = EmptySelectionOps {
            clipboard: RefCell::new(None),
        };

        let captured = capture_with(&ops).unwrap();

        assert_eq!(captured.text, "copied via cmd+c");
        assert_eq!(captured.source, CaptureSource::Clipboard);
    }
}
