// Pure TS mirror of `app/src-tauri/src/command_parser.rs`'s `parse` — the
// inline command syntax a user can embed in a captured selection: `/rd
// <direction>`, `/m <message>`, `/q <quoted context>`, `/lang <code>`, and a
// preset trigger `/<preset>`. Kept in lockstep with the Rust implementation's
// semantics (tag-hit detection rules, reserved tags, message assembly order)
// so the Capture panel's live-parse preview (`capture-preview`, B6) shows the
// user exactly what the backend parser will do — but this module never runs
// the actual refine pipeline; the Rust `command_parser` remains the single
// source of truth there.
//
// Only the fields the preview needs are mirrored (no byte/char-boundary
// bookkeeping beyond what JS string indexing already gives us); behavior for
// every case in `command_parser.rs`'s own test suite is asserted 1:1 in
// `command-preview.test.ts`.

/** The result of parsing a selection's inline commands. Mirrors Rust's
 * `ParsedCommand`, with `Option<String>` fields as `string | null`. */
export interface ParsedCommand {
  /** The `/rd` direction override, if present. */
  direction: string | null;
  /** The text to refine: the `/m` content, plus any untagged text (a
   * leading block before the first tag, and/or text trailing a preset
   * trigger). With no tags at all, this is the whole (trimmed) selection. */
  message: string;
  /** The explicit `/q` quoted-context override, if present. */
  quote: string | null;
  /** The `/lang` target language code, if present. */
  lang: string | null;
  /** The preset trigger name (without the leading `/`), if the selection
   * starts with a slash-word that isn't one of the reserved tags. */
  preset: string | null;
}

/** Tag words that are never treated as a preset trigger. */
const RESERVED_TAGS = new Set(['rd', 'm', 'q', 'lang']);

/** One recognized `/word` occurrence in the raw selection. */
interface TagHit {
  /** Index of the leading `/`. */
  start: number;
  /** Index just past the tag word (where its content begins). */
  contentStart: number;
  /** The tag word as written (without the `/`), original case. */
  word: string;
}

function isTagWordChar(ch: string): boolean {
  return /^[A-Za-z0-9_-]$/.test(ch);
}

function isWhitespace(ch: string | undefined): boolean {
  return ch !== undefined && /\s/.test(ch);
}

/** Finds every `/word` in `s` that looks like a tag: the `/` sits at the
 * start of the string or right after whitespace, and the word (ASCII
 * letters/digits/`-`/`_`) is immediately followed by whitespace or the end
 * of the string. Mirrors `command_parser.rs`'s `find_tag_hits`. */
function findTagHits(s: string): TagHit[] {
  const hits: TagHit[] = [];

  for (let idx = 0; idx < s.length; idx++) {
    if (s[idx] !== '/') continue;

    const precededOk = idx === 0 || isWhitespace(s[idx - 1]);
    if (!precededOk) continue;

    let end = idx + 1;
    while (end < s.length && isTagWordChar(s[end])) end++;
    const wordLen = end - (idx + 1);
    if (wordLen === 0) continue;

    const contentStart = idx + 1 + wordLen;
    const followedOk = contentStart === s.length || isWhitespace(s[contentStart]);
    if (!followedOk) continue;

    hits.push({ start: idx, contentStart, word: s.slice(idx + 1, contentStart) });
  }

  return hits;
}

/** Parses `selection`'s inline commands into a {@link ParsedCommand}.
 * Mirrors `command_parser.rs`'s `parse`. */
export function parseCommandPreview(selection: string): ParsedCommand {
  const hits = findTagHits(selection);

  if (hits.length === 0) {
    return { direction: null, message: selection.trim(), quote: null, lang: null, preset: null };
  }

  let direction: string | null = null;
  let quote: string | null = null;
  let lang: string | null = null;
  let preset: string | null = null;
  const messageParts: string[] = [];

  const leading = selection.slice(0, hits[0].start).trim();
  if (leading) messageParts.push(leading);

  for (let i = 0; i < hits.length; i++) {
    const hit = hits[i];
    const contentEnd = i + 1 < hits.length ? hits[i + 1].start : selection.length;
    const content = selection.slice(hit.contentStart, contentEnd).trim();
    const wordLower = hit.word.toLowerCase();

    switch (wordLower) {
      case 'rd':
        if (content) direction = content;
        break;
      case 'm':
        if (content) messageParts.push(content);
        break;
      case 'q':
        if (content) quote = content;
        break;
      case 'lang': {
        // Only the first whitespace-delimited token is the language code;
        // anything after it folds into the message rather than being
        // silently dropped.
        const spaceIdx = content.search(/\s/);
        const code = spaceIdx === -1 ? content : content.slice(0, spaceIdx);
        if (code) lang = code;
        const rest = (spaceIdx === -1 ? '' : content.slice(spaceIdx + 1)).trim();
        if (rest) messageParts.push(rest);
        break;
      }
      default:
        if (!RESERVED_TAGS.has(wordLower)) {
          if (preset === null) {
            // First preset trigger wins; its own content folds into the
            // message untagged.
            preset = hit.word;
            if (content) messageParts.push(content);
          } else {
            // A later non-reserved slash-word isn't a second preset trigger
            // — keep it (and its content) as literal message text rather
            // than silently dropping it.
            const literal = selection.slice(hit.start, contentEnd).trim();
            if (literal) messageParts.push(literal);
          }
        }
    }
  }

  return {
    direction,
    message: messageParts.join('\n'),
    quote,
    lang,
    preset,
  };
}
