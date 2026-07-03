//! Pure text-processing helpers for the Windows UI Automation backend
//! ([`crate::windows`]).
//!
//! Kept in their own module, separate from `windows.rs`, specifically so
//! they compile and run under `cfg(test)` on every OS (see the `mod`
//! declaration in `lib.rs`): `windows.rs` itself is gated to
//! `cfg(target_os = "windows")` since it makes real UI Automation/SendInput/
//! clipboard calls, and none of that can be built or exercised on this
//! Linux dev host. The non-trivial text munging UIA needs -- normalizing
//! its line-break quirk and combining a possibly-disjoint selection into
//! one string -- has no OS dependency, so it lives here where it can
//! actually be unit tested.

/// UI Automation text ranges use a lone `\r` (not `\r\n` or `\n`) as their
/// paragraph/line-break character in most edit controls -- a
/// long-documented UIA/Win32 quirk. Normalize both `\r\n` and lone `\r` to
/// `\n` so callers see consistent line endings regardless of the source
/// app or control.
pub(crate) fn normalize_uia_line_endings(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// A UIA text selection can consist of more than one disjoint range (e.g.
/// a column/box selection in some editors, or multiple carets). Combine
/// each range's own text (already read via `IUIAutomationTextRange::
/// GetText`) into the single logical selection string `capture_with`
/// expects back from [`crate::PlatformOps::ax_read_selection`], and
/// normalize the UIA line-break quirk along the way.
pub(crate) fn selection_text_from_ranges(ranges: &[String]) -> String {
    normalize_uia_line_endings(&ranges.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_crlf_and_lone_cr_to_lf() {
        assert_eq!(
            normalize_uia_line_endings("first\r\nsecond\rthird"),
            "first\nsecond\nthird"
        );
    }

    #[test]
    fn normalize_leaves_existing_lf_alone() {
        assert_eq!(normalize_uia_line_endings("a\nb"), "a\nb");
    }

    #[test]
    fn normalize_handles_text_with_no_line_breaks() {
        assert_eq!(normalize_uia_line_endings("just one line"), "just one line");
    }

    #[test]
    fn selection_text_joins_multiple_ranges_and_normalizes() {
        let ranges = vec!["first line\r".to_string(), "second line".to_string()];

        assert_eq!(
            selection_text_from_ranges(&ranges),
            "first line\nsecond line"
        );
    }

    #[test]
    fn selection_text_from_single_range_is_unchanged_besides_normalization() {
        let ranges = vec!["only one selection\r\nrange".to_string()];

        assert_eq!(
            selection_text_from_ranges(&ranges),
            "only one selection\nrange"
        );
    }

    #[test]
    fn selection_text_from_no_ranges_is_empty() {
        let ranges: Vec<String> = vec![];

        assert_eq!(selection_text_from_ranges(&ranges), "");
    }
}
