// Reducing a model's reply to just the rewritten text.
//
// A refine injects its result straight into whatever the user was typing in,
// so anything the model adds around the rewrite lands in their document. In
// practice models editorialise anyway — most often a trailing note explaining
// the edit:
//
//     I'm not sure. It seems they are quite busy, but this probably needs to
//     be discussed with Bryan.
//
//     **What changed:** The original had a comma splice joining ...
//
// The whole of that was being pasted over the user's selection.
//
// Two layers guard against it, because neither is sufficient alone:
//
//   1. `prompt_builder` asks for the rewrite wrapped in `<refined>` tags. A
//      delimiter is far more reliably honoured than "no commentary please",
//      and it survives a custom `/rd` direction or a preset — which replace
//      the default direction wholesale and so used to drop the only
//      instruction telling the model to keep quiet.
//   2. This module. When the tags are there, take what is between them. When
//      they aren't — an older or less compliant model — fall back to trimming
//      the shapes commentary actually takes.
//
// The fallback is deliberately conservative. Silently deleting a line the user
// wanted is worse than leaving a line they didn't, so it only strips text that
// matches a narrow set of commentary lead-ins, and only when it trails the
// rewrite rather than being the whole reply.

/// Tag the model is asked to wrap its rewrite in.
pub const OPEN_TAG: &str = "<refined>";
pub const CLOSE_TAG: &str = "</refined>";

/// Lead-ins that mark a trailing block as commentary rather than rewritten
/// text. Matched at the start of a line, case-insensitively, after optional
/// markdown bold/emphasis markers.
const COMMENTARY_LEAD_INS: &[&str] = &[
    "what changed",
    "changes:",
    "changes made",
    "note:",
    "notes:",
    "explanation",
    "why:",
    "why this",
    "reasoning",
    "i changed",
    "i've changed",
    "i have changed",
    "key changes",
];

/// Prefixes a model sometimes puts *before* the rewrite ("Here's the corrected
/// text:"). Only stripped when the line ends in a colon and something follows.
const PREAMBLE_LEAD_INS: &[&str] = &[
    "here's the",
    "here is the",
    "corrected text",
    "revised text",
    "rewritten text",
    "refined text",
    "sure,",
    "sure!",
];

/// Reduces a raw model reply to the text that should be injected.
pub fn extract_refined(raw: &str) -> String {
    // 1. The delimited happy path. Take the *first* open and the *last* close
    //    so a rewrite that itself mentions the tag can't truncate the result.
    if let Some(inner) = between_tags(raw) {
        return strip_wrapping_quotes(strip_code_fence(inner.trim())).to_string();
    }

    let mut text = strip_code_fence(raw.trim()).to_string();
    text = strip_preamble(&text);
    text = strip_trailing_commentary(&text);
    strip_wrapping_quotes(text.trim()).to_string()
}

fn between_tags(raw: &str) -> Option<&str> {
    let start = raw.find(OPEN_TAG)? + OPEN_TAG.len();
    let end = raw.rfind(CLOSE_TAG)?;
    if end < start {
        return None;
    }
    Some(&raw[start..end])
}

/// Removes a markdown fence wrapping the whole reply. Models fence prose
/// surprisingly often, and the backticks would be pasted verbatim.
fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let Some(after_open) = trimmed.find('\n') else {
        return text;
    };
    let body = &trimmed[after_open + 1..];
    match body.rfind("```") {
        // Only when the closing fence ends the reply — a fence in the middle
        // is content, not a wrapper.
        Some(close) if body[close + 3..].trim().is_empty() => body[..close].trim_end(),
        _ => text,
    }
}

/// Drops a wrapping pair of quotes the model added around the whole rewrite.
fn strip_wrapping_quotes(text: &str) -> &str {
    let pairs = [('"', '"'), ('\u{201c}', '\u{201d}'), ('\'', '\'')];
    for (open, close) in pairs {
        if text.len() > 1 && text.starts_with(open) && text.ends_with(close) {
            let inner = &text[open.len_utf8()..text.len() - close.len_utf8()];
            // Only when the quotes wrap the *whole* thing: a reply that merely
            // contains quotes must be left alone.
            if !inner.contains(close) {
                return inner;
            }
        }
    }
    text
}

/// Normalises a line for lead-in matching: lowercased, with markdown emphasis
/// and list markers removed.
fn normalise(line: &str) -> String {
    line.trim()
        .trim_start_matches(['-', '*', '#', ' '])
        .replace(['*', '_', '`'], "")
        .trim()
        .to_lowercase()
}

fn strip_preamble(text: &str) -> String {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return text.to_string();
    };
    let rest: Vec<&str> = lines.collect();
    let normalised = normalise(first);
    let looks_like_preamble = normalised.ends_with(':')
        && PREAMBLE_LEAD_INS
            .iter()
            .any(|lead| normalised.starts_with(lead));
    // Never strip it if that would leave nothing.
    if looks_like_preamble && rest.iter().any(|l| !l.trim().is_empty()) {
        return rest.join("\n").trim_start().to_string();
    }
    text.to_string()
}

fn strip_trailing_commentary(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    // Walk backwards for the earliest commentary lead-in that still leaves
    // some rewrite in front of it.
    let mut cut: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let normalised = normalise(line);
        if normalised.is_empty() {
            continue;
        }
        let is_lead_in = COMMENTARY_LEAD_INS
            .iter()
            .any(|lead| normalised.starts_with(lead));
        if !is_lead_in {
            continue;
        }
        // Must not be the whole reply: something non-empty has to precede it.
        if lines[..i].iter().any(|l| !l.trim().is_empty()) {
            cut = Some(i);
            break;
        }
    }

    match cut {
        Some(i) => lines[..i].join("\n").trim_end().to_string(),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_what_is_between_the_tags() {
        let raw = "<refined>I believe this works.</refined>";
        assert_eq!(extract_refined(raw), "I believe this works.");
    }

    #[test]
    fn ignores_chatter_outside_the_tags() {
        let raw = "Sure! Here you go:\n<refined>Tightened up.</refined>\n\nHope that helps!";
        assert_eq!(extract_refined(raw), "Tightened up.");
    }

    #[test]
    fn a_rewrite_mentioning_the_tag_is_not_truncated() {
        let raw = "<refined>Use the </refined> marker carefully.</refined>";
        assert_eq!(extract_refined(raw), "Use the </refined> marker carefully.");
    }

    /// The exact reply that prompted this: a clean rewrite followed by a
    /// bolded explanation, all of it pasted into the user's message box.
    #[test]
    fn strips_the_trailing_what_changed_note() {
        let raw = "I'm not sure. It seems they are quite busy, but this probably needs to be \
discussed with Bryan.\n\n**What changed:** The original had a comma splice joining \"I'm not \
sure\" and \"it seems they are quite busy,\" which are two independent clauses. Splitting them \
into separate sentences fixes it (a semicolon would also work if you prefer keeping it as one \
sentence).";
        assert_eq!(
            extract_refined(raw),
            "I'm not sure. It seems they are quite busy, but this probably needs to be discussed \
with Bryan."
        );
    }

    #[test]
    fn strips_other_commentary_headings() {
        for heading in [
            "**Changes:** tightened it",
            "Note: I shortened the second clause",
            "*Explanation* — comma splice",
            "- Key changes: split the sentence",
            "Why: it read as a run-on",
        ] {
            let raw = format!("The rewritten sentence.\n\n{heading}");
            assert_eq!(
                extract_refined(&raw),
                "The rewritten sentence.",
                "failed for heading: {heading}"
            );
        }
    }

    #[test]
    fn strips_a_leading_preamble() {
        let raw = "Here's the corrected text:\n\nI believe this works.";
        assert_eq!(extract_refined(raw), "I believe this works.");
    }

    #[test]
    fn unwraps_a_fenced_reply() {
        let raw = "```\nI believe this works.\n```";
        assert_eq!(extract_refined(raw), "I believe this works.");
        let tagged = "```markdown\nI believe this works.\n```";
        assert_eq!(extract_refined(tagged), "I believe this works.");
    }

    #[test]
    fn leaves_a_fence_in_the_middle_of_the_text_alone() {
        // Someone refining a message that quotes a code block must keep it.
        let raw = "Try this:\n\n```\ncargo build\n```\n\nThen run it.";
        assert_eq!(extract_refined(raw), raw);
    }

    #[test]
    fn drops_quotes_wrapping_the_whole_reply() {
        assert_eq!(extract_refined("\"I believe this works.\""), "I believe this works.");
        assert_eq!(extract_refined("\u{201c}Curly too.\u{201d}"), "Curly too.");
    }

    #[test]
    fn keeps_quotes_that_are_part_of_the_text() {
        let raw = "She said \"hello\" and left.";
        assert_eq!(extract_refined(raw), raw);
        let both = "\"Quoted\" and \"quoted again\"";
        assert_eq!(extract_refined(both), both);
    }

    #[test]
    fn a_plain_rewrite_is_returned_untouched() {
        let raw = "I believe this feature doesn't work correctly on the new build.";
        assert_eq!(extract_refined(raw), raw);
    }

    #[test]
    fn never_strips_away_the_entire_reply() {
        // A reply that is *only* a lead-in is more likely a legitimate rewrite
        // of text that happens to start that way than it is pure commentary.
        for raw in ["Note: bring your passport", "What changed since Tuesday?"] {
            assert_eq!(extract_refined(raw), raw);
        }
    }

    #[test]
    fn multi_paragraph_rewrites_survive() {
        let raw = "First paragraph stays.\n\nSecond paragraph stays too.\n\nAnd a third.";
        assert_eq!(extract_refined(raw), raw);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(extract_refined("\n\n  Tidy.  \n\n"), "Tidy.");
    }

    #[test]
    fn handles_an_empty_or_whitespace_reply() {
        assert_eq!(extract_refined(""), "");
        assert_eq!(extract_refined("   \n  "), "");
    }

    #[test]
    fn combines_tags_with_a_fence_inside_them() {
        let raw = "<refined>```\nFenced inside tags.\n```</refined>";
        assert_eq!(extract_refined(raw), "Fenced inside tags.");
    }
}
