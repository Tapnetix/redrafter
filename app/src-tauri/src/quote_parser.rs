// Heuristic detection of quoted-reply and signature blocks inside a
// message, so a caller (`prompt_builder`, via the future orchestrator
// wiring) can present that text to the model as reference-only context
// and refine only the user's own draft.
//
// This is a fallback for when the user hasn't marked the quote explicitly
// with `/q` (see `command_parser`) — `split` looks for the shapes a
// pasted email/chat thread commonly takes: lines starting with `>`, an
// `"On ... wrote:"` header introducing a quoted block, and a trailing
// `--` email-signature delimiter.
//
// Deliberately conservative: a bare closing salutation like `"thanks"` or
// `"Best,"` on its own line is *not* treated as a signature, even near the
// end of the message. That heuristic used to exist here, but it produced
// false positives — a short draft that legitimately ends in "thanks" or
// "Best,\nJane" would have that sign-off silently stripped out of the
// refined result. A false positive (losing part of the user's real draft)
// is worse than a false negative (occasionally leaving a real signature
// block in the draft), so only the unambiguous delimiters below are
// recognized.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/parser_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block — see `prompt_builder.rs`.)

/// Splits `text` into `(quoted_context, user_draft)`. `quoted_context` is
/// `None` when no quote-like or signature-like lines are detected, in
/// which case `user_draft` is the whole (trimmed) `text`, unchanged.
pub fn split(text: &str) -> (Option<String>, String) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (None, String::new());
    }

    let mut is_context = vec![false; lines.len()];

    // Reply-quote blocks: an "On ... wrote:" header optionally followed by
    // (blank or) '>' lines, or standalone '>' lines anywhere.
    let mut i = 0;
    while i < lines.len() {
        if is_wrote_header(lines[i]) {
            is_context[i] = true;
            i += 1;
            while i < lines.len() && (lines[i].trim().is_empty() || is_quote_line(lines[i])) {
                is_context[i] = true;
                i += 1;
            }
            continue;
        }
        if is_quote_line(lines[i]) {
            is_context[i] = true;
        }
        i += 1;
    }

    // Trailing signature block, if any.
    if let Some(sig_start) = find_signature_start(&lines) {
        for ctx in is_context.iter_mut().skip(sig_start) {
            *ctx = true;
        }
    }

    let quoted: Vec<&str> = lines
        .iter()
        .zip(is_context.iter())
        .filter(|(_, ctx)| **ctx)
        .map(|(line, _)| *line)
        .collect();
    let draft: Vec<&str> = lines
        .iter()
        .zip(is_context.iter())
        .filter(|(_, ctx)| !**ctx)
        .map(|(line, _)| *line)
        .collect();

    let quoted_context = if quoted.is_empty() {
        None
    } else {
        Some(quoted.join("\n").trim().to_string())
    };
    (quoted_context, draft.join("\n").trim().to_string())
}

/// A line that's part of a quoted block: `>`, optionally preceded by
/// whitespace, and allowing nested `>>` quoting.
fn is_quote_line(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// An email/chat-client-style quote header, e.g. `"On Mon, Alex wrote:"`.
fn is_wrote_header(line: &str) -> bool {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("on ") && lower.ends_with("wrote:")
}

/// Finds the line index where a trailing signature block starts, if any:
/// the canonical `--` email-signature delimiter. See the module-level doc
/// comment for why a bare closing salutation is deliberately *not*
/// recognized here.
fn find_signature_start(lines: &[&str]) -> Option<usize> {
    lines.iter().position(|line| line.trim() == "--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_with_no_quote_markers_is_returned_unchanged_as_the_draft() {
        let (quote, draft) = split("just a normal message with no quoting");

        assert_eq!(quote, None);
        assert_eq!(draft, "just a normal message with no quoting");
    }

    #[test]
    fn leading_gt_prefixed_lines_are_detected_as_the_quote() {
        let text = "> hey are we still on track?\n> any risk of slipping?\n\nyes, all good";

        let (quote, draft) = split(text);

        assert_eq!(
            quote,
            Some("> hey are we still on track?\n> any risk of slipping?".to_string())
        );
        assert_eq!(draft, "yes, all good");
    }

    #[test]
    fn a_single_gt_prefixed_line_mixed_into_the_middle_of_a_message_is_still_detected() {
        let text = "we said\n> On Mon, Alex wrote: any risk of slipping?\nwe're on track";

        let (quote, draft) = split(text);

        assert_eq!(
            quote,
            Some("> On Mon, Alex wrote: any risk of slipping?".to_string())
        );
        assert_eq!(draft, "we said\nwe're on track");
    }

    #[test]
    fn on_wrote_header_followed_by_quote_lines_is_detected_as_one_block() {
        let text = "On Mon, Jan 5, 2026, Alex Smith wrote:\n\
                     > Hey are we still on track?\n\
                     > Any risk of slipping?\n\
                     \n\
                     Yes, all good, shipping Monday.";

        let (quote, draft) = split(text);

        assert_eq!(
            quote,
            Some(
                "On Mon, Jan 5, 2026, Alex Smith wrote:\n\
                 > Hey are we still on track?\n\
                 > Any risk of slipping?"
                    .to_string()
            )
        );
        assert_eq!(draft, "Yes, all good, shipping Monday.");
    }

    #[test]
    fn trailing_double_dash_signature_delimiter_is_detected() {
        let text = "Thanks for checking in, we're on track.\n\n--\nJane Doe\nProduct Lead";

        let (quote, draft) = split(text);

        assert_eq!(quote, Some("--\nJane Doe\nProduct Lead".to_string()));
        assert_eq!(draft, "Thanks for checking in, we're on track.");
    }

    #[test]
    fn a_short_closing_salutation_near_the_end_is_not_treated_as_a_signature() {
        // Regression: this used to be stripped out as a "signature block",
        // which would silently drop a real sign-off from a short draft.
        // Only the unambiguous `--` delimiter (and `>`-quoting / "wrote:"
        // headers) count as context — see the module doc comment.
        let text = "Sounds good, see you then.\n\nBest,\nJane";

        let (quote, draft) = split(text);

        assert_eq!(quote, None);
        assert_eq!(draft, text);
    }

    #[test]
    fn a_trailing_bare_thanks_is_not_treated_as_a_signature() {
        let text = "hey can you send the deck\n\nthanks";

        let (quote, draft) = split(text);

        assert_eq!(quote, None);
        assert_eq!(draft, text);
    }

    #[test]
    fn a_leading_quote_and_a_trailing_signature_are_both_pulled_out() {
        let text = "> On Mon, Alex wrote: any risk of slipping?\n\
                     \n\
                     No delays, shipping Monday.\n\
                     \n\
                     --\n\
                     Jane Doe";

        let (quote, draft) = split(text);

        let quote = quote.expect("both the leading quote and trailing signature are context");
        assert!(quote.contains("Alex wrote"));
        assert!(quote.contains("Jane Doe"));
        assert_eq!(draft, "No delays, shipping Monday.");
    }

    #[test]
    fn a_salutation_word_that_is_not_on_its_own_line_is_not_treated_as_a_signature() {
        let text = "Best wishes for the launch, thanks for the update.";

        let (quote, draft) = split(text);

        assert_eq!(quote, None);
        assert_eq!(draft, text);
    }
}
