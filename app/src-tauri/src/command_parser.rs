// Parses the inline command syntax a user can embed in a captured
// selection: `/rd <direction>`, `/m <message>`, `/q <quoted context>`,
// `/lang <code>`, and a preset trigger `/<preset>`. Tags may appear in
// any order, mixed with untagged text; `parse` never destroys the
// user's own words — everything not claimed by `/rd`, `/q`, or `/lang`
// (including a leading untagged block, and the text after a preset
// trigger) becomes the message to refine.
//
// Heuristic quoted-context detection (lines starting `>`, `"On ... wrote:"`
// headers, signature blocks) is a separate concern — see `quote_parser`.
// `parse` only recognizes an *explicit* `/q` tag; anything else stays in
// `message` for a caller to run through `quote_parser::split` if desired.
//
// Preset *resolution* (looking up a trigger's stored direction/overrides)
// is a later phase (C17); `parse` only extracts the trigger name.
//
// (Plain `//` rather than a `//!` module doc: this file is also pulled into
// `tests/parser_test.rs` via `include!` inside an inline `mod` block, and
// Rust doesn't allow an inner doc comment produced by macro expansion to
// sit at the start of that block — see `prompt_builder.rs`.)

/// The result of parsing a selection's inline commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCommand {
    /// The `/rd` direction override, if present.
    pub direction: Option<String>,
    /// The text to refine: the `/m` content if present, plus any untagged
    /// text (a leading block before the first tag, and/or the text
    /// trailing a preset trigger). With no tags at all, this is the whole
    /// (trimmed) selection.
    pub message: String,
    /// The explicit `/q` quoted-context override, if present.
    pub quote: Option<String>,
    /// The `/lang` target language code, if present.
    pub lang: Option<String>,
    /// The preset trigger name (without the leading `/`), if the selection
    /// starts with a slash-word that isn't one of the reserved tags.
    pub preset: Option<String>,
}

/// One recognized `/word` occurrence in the raw selection: its tag word
/// (lowercased for matching), the byte offset where its content begins,
/// and (for reserved tags only, to preserve original casing on presets)
/// the original word text.
struct TagHit<'a> {
    /// Byte offset of the leading `/`.
    start: usize,
    /// Byte offset just past the tag word (where its content begins).
    content_start: usize,
    /// The tag word as written (without the `/`), original case.
    word: &'a str,
}

/// Reserved tag words that are never treated as a preset trigger.
fn reserved_tag(word_lower: &str) -> bool {
    matches!(word_lower, "rd" | "m" | "q" | "lang")
}

/// Finds every `/word` in `s` that looks like a tag: the `/` sits at the
/// start of the string or right after whitespace, and the word (ASCII
/// letters/digits/`-`/`_`) is immediately followed by whitespace or the
/// end of the string. This keeps things like a bare URL fragment or a
/// path containing `/` from being mistaken for a tag.
fn find_tag_hits(s: &str) -> Vec<TagHit<'_>> {
    let mut hits = Vec::new();

    for (idx, ch) in s.char_indices() {
        if ch != '/' {
            continue;
        }
        let preceded_ok = idx == 0
            || s[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        if !preceded_ok {
            continue;
        }

        let rest = &s[idx + ch.len_utf8()..];
        let word_len_bytes: usize = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .map(|c| c.len_utf8())
            .sum();
        if word_len_bytes == 0 {
            continue;
        }

        let content_start = idx + ch.len_utf8() + word_len_bytes;
        let followed_ok = content_start == s.len()
            || s[content_start..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace());
        if !followed_ok {
            continue;
        }

        hits.push(TagHit {
            start: idx,
            content_start,
            word: &s[idx + ch.len_utf8()..content_start],
        });
    }

    hits
}

/// Parses `selection`'s inline commands into a [`ParsedCommand`].
pub fn parse(selection: &str) -> ParsedCommand {
    let hits = find_tag_hits(selection);

    if hits.is_empty() {
        return ParsedCommand {
            message: selection.trim().to_string(),
            ..Default::default()
        };
    }

    let mut direction = None;
    let mut quote = None;
    let mut lang = None;
    let mut preset = None;
    let mut message_parts: Vec<&str> = Vec::new();

    let leading = selection[..hits[0].start].trim();
    if !leading.is_empty() {
        message_parts.push(leading);
    }

    for (i, hit) in hits.iter().enumerate() {
        let content_end = hits.get(i + 1).map(|h| h.start).unwrap_or(selection.len());
        let content = selection[hit.content_start..content_end].trim();
        let word_lower = hit.word.to_ascii_lowercase();

        match word_lower.as_str() {
            "rd" => {
                if !content.is_empty() {
                    direction = Some(content.to_string());
                }
            }
            "m" => {
                if !content.is_empty() {
                    message_parts.push(content);
                }
            }
            "q" => {
                if !content.is_empty() {
                    quote = Some(content.to_string());
                }
            }
            "lang" => {
                // Only the first token is the language code; anything
                // after it has nowhere else to go, so it folds into the
                // message rather than being silently dropped.
                let mut words = content.splitn(2, char::is_whitespace);
                if let Some(code) = words.next().filter(|c| !c.is_empty()) {
                    lang = Some(code.to_string());
                }
                if let Some(rest) = words.next() {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        message_parts.push(rest);
                    }
                }
            }
            _ => {
                if !reserved_tag(&word_lower) {
                    if preset.is_none() {
                        // First preset trigger wins; its own content
                        // folds into the message untagged (see
                        // `untagged_text_around_a_preset_trigger_is_folded_into_the_message`).
                        preset = Some(hit.word.to_string());
                        if !content.is_empty() {
                            message_parts.push(content);
                        }
                    } else {
                        // A later non-reserved slash-word isn't a second
                        // preset trigger — only the first one is honored.
                        // Keep the slash-word itself (and its content) as
                        // literal message text, sliced verbatim from the
                        // original selection, rather than silently
                        // dropping it — a second `/word` the user typed
                        // shouldn't vanish from the refined text.
                        let literal = selection[hit.start..content_end].trim();
                        if !literal.is_empty() {
                            message_parts.push(literal);
                        }
                    }
                }
            }
        }
    }

    ParsedCommand {
        direction,
        message: message_parts.join("\n"),
        quote,
        lang,
        preset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tags_the_whole_selection_is_the_message() {
        let parsed = parse("we was gonna ship fri, no biggie");

        assert_eq!(parsed.message, "we was gonna ship fri, no biggie");
        assert_eq!(parsed.direction, None);
        assert_eq!(parsed.quote, None);
        assert_eq!(parsed.lang, None);
        assert_eq!(parsed.preset, None);
    }

    #[test]
    fn rd_tag_alone_sets_direction_and_leaves_no_message() {
        let parsed = parse("/rd make it more formal");

        assert_eq!(parsed.direction, Some("make it more formal".to_string()));
        assert_eq!(parsed.message, "");
        assert_eq!(parsed.quote, None);
    }

    #[test]
    fn m_tag_alone_sets_the_message_and_no_direction() {
        let parsed = parse("/m we was gonna ship fri");

        assert_eq!(parsed.message, "we was gonna ship fri");
        assert_eq!(parsed.direction, None);
    }

    #[test]
    fn q_tag_alone_sets_the_explicit_quote() {
        let parsed = parse("/q On Mon, Alex wrote: any risk of slipping?");

        assert_eq!(
            parsed.quote,
            Some("On Mon, Alex wrote: any risk of slipping?".to_string())
        );
        assert_eq!(parsed.message, "");
    }

    #[test]
    fn lang_tag_alone_sets_the_language_code() {
        let parsed = parse("/lang de");

        assert_eq!(parsed.lang, Some("de".to_string()));
    }

    #[test]
    fn lang_tag_only_takes_the_first_whitespace_token() {
        // Guards against a stray trailing word being folded into the
        // language code when there's no next tag to bound it.
        let parsed = parse("/lang de please");

        assert_eq!(parsed.lang, Some("de".to_string()));
        assert_eq!(parsed.message, "please");
    }

    #[test]
    fn preset_trigger_alone_captures_the_trigger_and_trailing_text_as_message() {
        let parsed = parse("/formal attached is the report");

        assert_eq!(parsed.preset, Some("formal".to_string()));
        assert_eq!(parsed.message, "attached is the report");
        assert_eq!(parsed.direction, None);
    }

    #[test]
    fn rd_and_m_combined_in_the_designs_order() {
        let selection = "/rd read the below /m we was gonna ship fri, no delays";
        let parsed = parse(selection);

        assert_eq!(parsed.direction, Some("read the below".to_string()));
        assert_eq!(parsed.message, "we was gonna ship fri, no delays");
        assert!(!parsed.message.contains("/rd"));
        assert!(!parsed.direction.unwrap().contains("/m"));
    }

    #[test]
    fn tags_in_any_order_message_before_direction() {
        let selection = "/m we was gonna ship fri /rd make it formal";
        let parsed = parse(selection);

        assert_eq!(parsed.direction, Some("make it formal".to_string()));
        assert_eq!(parsed.message, "we was gonna ship fri");
    }

    #[test]
    fn all_four_reserved_tags_combined_in_mixed_order() {
        let selection =
            "/lang de /q Alex wrote: any risk? /rd keep it warm but concise /m we're on track";
        let parsed = parse(selection);

        assert_eq!(parsed.lang, Some("de".to_string()));
        assert_eq!(parsed.quote, Some("Alex wrote: any risk?".to_string()));
        assert_eq!(
            parsed.direction,
            Some("keep it warm but concise".to_string())
        );
        assert_eq!(parsed.message, "we're on track");
    }

    #[test]
    fn leading_untagged_text_before_the_first_tag_becomes_message() {
        let selection = "> On Mon, Alex wrote: are we still on track?\n\
                          /rd keep it warm /m we're good, shipping Monday";
        let parsed = parse(selection);

        assert_eq!(parsed.direction, Some("keep it warm".to_string()));
        assert_eq!(
            parsed.message,
            "> On Mon, Alex wrote: are we still on track?\nwe're good, shipping Monday"
        );
        assert_eq!(
            parsed.quote, None,
            "no explicit /q — this is quote_parser's job"
        );
    }

    #[test]
    fn untagged_text_around_a_preset_trigger_is_folded_into_the_message() {
        let selection = "hello /greet world";
        let parsed = parse(selection);

        assert_eq!(parsed.preset, Some("greet".to_string()));
        assert_eq!(parsed.message, "hello\nworld");
    }

    #[test]
    fn a_second_non_reserved_slash_word_does_not_become_a_second_preset_and_is_not_dropped() {
        // Only the first preset trigger is honored; a later slash-word is
        // literal message text, not silently swallowed.
        let selection = "/foo hello /bar world";
        let parsed = parse(selection);

        assert_eq!(parsed.preset, Some("foo".to_string()));
        assert_eq!(parsed.message, "hello\n/bar world");
    }

    #[test]
    fn a_slash_not_followed_by_whitespace_is_not_mistaken_for_a_tag() {
        // e.g. a pasted file path shouldn't be misread as a preset trigger.
        let selection = "check /usr/bin/ls for the binary";
        let parsed = parse(selection);

        assert_eq!(parsed.preset, None);
        assert_eq!(parsed.message, selection);
    }

    #[test]
    fn tag_words_are_case_insensitive() {
        let parsed = parse("/RD be concise /M hello there");

        assert_eq!(parsed.direction, Some("be concise".to_string()));
        assert_eq!(parsed.message, "hello there");
    }
}
