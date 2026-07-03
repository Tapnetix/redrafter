import { describe, expect, it } from 'vitest';
import { parseCommandPreview } from './command-preview';

// Mirrors app/src-tauri/src/command_parser.rs's own unit tests (see that
// file's `#[cfg(test)] mod tests`) so this pure TS mirror's behavior stays in
// lockstep with the authoritative Rust parser for every tag combo.
describe('parseCommandPreview', () => {
  it('with no tags, the whole selection is the message', () => {
    const parsed = parseCommandPreview('we was gonna ship fri, no biggie');

    expect(parsed.message).toBe('we was gonna ship fri, no biggie');
    expect(parsed.direction).toBeNull();
    expect(parsed.quote).toBeNull();
    expect(parsed.lang).toBeNull();
    expect(parsed.preset).toBeNull();
  });

  it('/rd tag alone sets direction and leaves no message', () => {
    const parsed = parseCommandPreview('/rd make it more formal');

    expect(parsed.direction).toBe('make it more formal');
    expect(parsed.message).toBe('');
    expect(parsed.quote).toBeNull();
  });

  it('/m tag alone sets the message and no direction', () => {
    const parsed = parseCommandPreview('/m we was gonna ship fri');

    expect(parsed.message).toBe('we was gonna ship fri');
    expect(parsed.direction).toBeNull();
  });

  it('/q tag alone sets the explicit quote', () => {
    const parsed = parseCommandPreview('/q On Mon, Alex wrote: any risk of slipping?');

    expect(parsed.quote).toBe('On Mon, Alex wrote: any risk of slipping?');
    expect(parsed.message).toBe('');
  });

  it('/lang tag alone sets the language code', () => {
    const parsed = parseCommandPreview('/lang de');

    expect(parsed.lang).toBe('de');
  });

  it('/lang tag only takes the first whitespace token', () => {
    const parsed = parseCommandPreview('/lang de please');

    expect(parsed.lang).toBe('de');
    expect(parsed.message).toBe('please');
  });

  it('a preset trigger alone captures the trigger and trailing text as message', () => {
    const parsed = parseCommandPreview('/formal attached is the report');

    expect(parsed.preset).toBe('formal');
    expect(parsed.message).toBe('attached is the report');
    expect(parsed.direction).toBeNull();
  });

  it('/rd and /m combined in the design’s order', () => {
    const selection = '/rd read the below /m we was gonna ship fri, no delays';
    const parsed = parseCommandPreview(selection);

    expect(parsed.direction).toBe('read the below');
    expect(parsed.message).toBe('we was gonna ship fri, no delays');
    expect(parsed.message).not.toContain('/rd');
    expect(parsed.direction).not.toContain('/m');
  });

  it('tags in any order: message before direction', () => {
    const selection = '/m we was gonna ship fri /rd make it formal';
    const parsed = parseCommandPreview(selection);

    expect(parsed.direction).toBe('make it formal');
    expect(parsed.message).toBe('we was gonna ship fri');
  });

  it('all four reserved tags combined in mixed order', () => {
    const selection =
      '/lang de /q Alex wrote: any risk? /rd keep it warm but concise /m we\'re on track';
    const parsed = parseCommandPreview(selection);

    expect(parsed.lang).toBe('de');
    expect(parsed.quote).toBe('Alex wrote: any risk?');
    expect(parsed.direction).toBe('keep it warm but concise');
    expect(parsed.message).toBe("we're on track");
  });

  it('leading untagged text before the first tag becomes message', () => {
    const selection =
      '> On Mon, Alex wrote: are we still on track?\n/rd keep it warm /m we\'re good, shipping Monday';
    const parsed = parseCommandPreview(selection);

    expect(parsed.direction).toBe('keep it warm');
    expect(parsed.message).toBe(
      "> On Mon, Alex wrote: are we still on track?\nwe're good, shipping Monday",
    );
    expect(parsed.quote).toBeNull();
  });

  it('untagged text around a preset trigger is folded into the message', () => {
    const parsed = parseCommandPreview('hello /greet world');

    expect(parsed.preset).toBe('greet');
    expect(parsed.message).toBe('hello\nworld');
  });

  it('a second non-reserved slash-word does not become a second preset and is not dropped', () => {
    const parsed = parseCommandPreview('/foo hello /bar world');

    expect(parsed.preset).toBe('foo');
    expect(parsed.message).toBe('hello\n/bar world');
  });

  it('a slash not followed by whitespace is not mistaken for a tag', () => {
    const selection = 'check /usr/bin/ls for the binary';
    const parsed = parseCommandPreview(selection);

    expect(parsed.preset).toBeNull();
    expect(parsed.message).toBe(selection);
  });

  it('tag words are case-insensitive', () => {
    const parsed = parseCommandPreview('/RD be concise /M hello there');

    expect(parsed.direction).toBe('be concise');
    expect(parsed.message).toBe('hello there');
  });

  it('an empty selection has no tags and an empty message', () => {
    const parsed = parseCommandPreview('');

    expect(parsed.message).toBe('');
    expect(parsed.direction).toBeNull();
    expect(parsed.quote).toBeNull();
    expect(parsed.lang).toBeNull();
    expect(parsed.preset).toBeNull();
  });
});
