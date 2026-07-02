use crate::PlatformOps;
use anyhow::Result;

/// AX-first-with-verify, clipboard save/restore fallback injection.
/// Generic over [`PlatformOps`] so it can be unit tested without touching
/// the real OS.
///
/// AX is tried first: write the selection, then read it back to verify
/// the write actually landed (some apps silently ignore AXValue writes on
/// read-only or unsupported controls). If the write or the verify fails,
/// fall back to the clipboard: save the user's current clipboard, write
/// the new text, paste it in, verify the clipboard held what we set, then
/// restore the user's prior clipboard contents regardless of the verify
/// outcome — refine must never leave the clipboard clobbered.
pub fn inject_with<P: PlatformOps>(ops: &P, text: &str) -> Result<()> {
    if try_ax_inject(ops, text) {
        return Ok(());
    }
    inject_via_clipboard(ops, text)
}

fn try_ax_inject<P: PlatformOps>(ops: &P, text: &str) -> bool {
    if ops.ax_write_selection(text).is_err() {
        return false;
    }
    matches!(ops.ax_read_selection(), Ok(readback) if readback == text)
}

fn inject_via_clipboard<P: PlatformOps>(ops: &P, text: &str) -> Result<()> {
    let saved = ops.clipboard_get().unwrap_or(None);

    // Run the write+paste+verify as a single fallible step so a hard
    // error from `clipboard_set`/`simulate_paste` doesn't skip the
    // restore below via early `?` return — the prior clipboard must be
    // restored on every error branch, not just the verify-mismatch one.
    let result: Result<bool> = (|| {
        ops.clipboard_set(text)?;
        ops.simulate_paste()?;
        Ok(matches!(ops.clipboard_get(), Ok(Some(got)) if got == text))
    })();

    // Always restore the user's prior clipboard, even if the write/paste
    // hard-errored or verify failed — never leave the clipboard in a
    // state the user didn't put it in.
    if let Some(prior) = &saved {
        ops.clipboard_set(prior)?;
    }

    if result? {
        Ok(())
    } else {
        anyhow::bail!("clipboard fallback: verify read after write did not match")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::cell::RefCell;

    struct VerifyMismatchOps {
        clipboard: RefCell<Option<String>>,
    }

    impl PlatformOps for VerifyMismatchOps {
        fn ax_read_selection(&self) -> Result<String> {
            Err(anyhow!("AX unavailable"))
        }
        fn ax_write_selection(&self, _text: &str) -> Result<()> {
            Err(anyhow!("AX unavailable"))
        }
        fn clipboard_get(&self) -> Result<Option<String>> {
            Ok(self.clipboard.borrow().clone())
        }
        fn clipboard_set(&self, text: &str) -> Result<()> {
            *self.clipboard.borrow_mut() = Some(text.to_string());
            Ok(())
        }
        fn simulate_copy(&self) -> Result<()> {
            Ok(())
        }
        fn simulate_paste(&self) -> Result<()> {
            // Simulate something else racing in and overwriting the
            // clipboard before our verify read.
            *self.clipboard.borrow_mut() = Some("clobbered by another app".to_string());
            Ok(())
        }
    }

    #[test]
    fn clipboard_verify_failure_still_restores_prior_clipboard_and_errors() {
        let ops = VerifyMismatchOps {
            clipboard: RefCell::new(Some("prior clipboard".to_string())),
        };

        let result = inject_with(&ops, "new text");

        assert!(result.is_err());
        assert_eq!(
            ops.clipboard_get().unwrap(),
            Some("prior clipboard".to_string()),
            "prior clipboard must be restored even when verify fails"
        );
    }

    /// Fake that can be made to fail either the `clipboard_set` write of
    /// the new text, or the `simulate_paste` step, to exercise the hard
    /// error paths (as opposed to the verify-mismatch soft failure above).
    struct ClipboardErrorOps {
        clipboard: RefCell<Option<String>>,
        fail_set_of_new_text: bool,
        fail_paste: bool,
    }

    impl PlatformOps for ClipboardErrorOps {
        fn ax_read_selection(&self) -> Result<String> {
            Err(anyhow!("AX unavailable"))
        }
        fn ax_write_selection(&self, _text: &str) -> Result<()> {
            Err(anyhow!("AX unavailable"))
        }
        fn clipboard_get(&self) -> Result<Option<String>> {
            Ok(self.clipboard.borrow().clone())
        }
        fn clipboard_set(&self, text: &str) -> Result<()> {
            if self.fail_set_of_new_text && text == "new text" {
                return Err(anyhow!("clipboard set failed"));
            }
            *self.clipboard.borrow_mut() = Some(text.to_string());
            Ok(())
        }
        fn simulate_copy(&self) -> Result<()> {
            Ok(())
        }
        fn simulate_paste(&self) -> Result<()> {
            if self.fail_paste {
                return Err(anyhow!("simulate_paste failed"));
            }
            Ok(())
        }
    }

    #[test]
    fn clipboard_set_error_still_restores_prior_clipboard_and_errors() {
        let ops = ClipboardErrorOps {
            clipboard: RefCell::new(Some("prior clipboard".to_string())),
            fail_set_of_new_text: true,
            fail_paste: false,
        };

        let result = inject_with(&ops, "new text");

        assert!(result.is_err());
        assert_eq!(
            ops.clipboard_get().unwrap(),
            Some("prior clipboard".to_string()),
            "prior clipboard must be restored even when clipboard_set(text) hard-errors"
        );
    }

    #[test]
    fn simulate_paste_error_still_restores_prior_clipboard_and_errors() {
        let ops = ClipboardErrorOps {
            clipboard: RefCell::new(Some("prior clipboard".to_string())),
            fail_set_of_new_text: false,
            fail_paste: true,
        };

        let result = inject_with(&ops, "new text");

        assert!(result.is_err());
        assert_eq!(
            ops.clipboard_get().unwrap(),
            Some("prior clipboard".to_string()),
            "prior clipboard must be restored even when simulate_paste() hard-errors \
             after the clipboard was already overwritten with the new text"
        );
    }
}
