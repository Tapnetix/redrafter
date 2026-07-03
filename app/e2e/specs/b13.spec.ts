import { test, expect } from '../fixtures/setup';

/**
 * S4: Language override in prompt builder — given a selection with
 * `/lang de`, when refined, then the result is a polished German version of
 * the user's message. (design-redrafter.md, wireframes/capture.html,
 * controls/capture.json)
 *
 * B4 parses the `/lang <code>` tag and `prompt_builder` appends a
 * language-target instruction to the model's system message
 * (`app/src-tauri/src/prompt_builder.rs`); that Rust-side wiring is
 * exercised by its own unit tests. Phase A/B don't yet expose a
 * "peek at the current OS selection" backend query — Capture's
 * `capturedText` prop (B6) is a documented seam for a future
 * `capture-peek` command, not something the standalone `/capture` E2E
 * route (or the mocked `refine` IPC call, which takes no args) can drive
 * today. So this spec exercises the two things that *are* observable
 * end-to-end through the mocked Tauri IPC:
 *   - the always-shown `/lang` preview (`capture-preview-lang`) that
 *     documents the `/lang de` -> German mapping the user can type inline
 *     (`controls/capture.json`, `wireframes/capture.html`), and
 *   - the full refine round trip: `capture-refine` calls the single
 *     `refine` backend command and the German-localized result it hands
 *     back lands in the Done state, exactly like any other refine.
 */
test.describe('Language override in prompt builder', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineOutcome: {
        original: 'we good with the Q3 release plan i think, no delays i hope, shipping monday probably',
        refined: 'Der Q3-Releaseplan steht — keine Verzögerungen. Lieferung voraussichtlich am Montag.',
        model: 'claude-opus-4-6',
      },
    },
  });

  test('S4: a /lang command sets the output language for the refine', async ({ page }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();

    // The live parse preview documents the `/lang <code>` syntax and shows
    // the concrete `/lang de` -> German example the design calls out for S4.
    const langPreview = page.getByTestId('capture-preview-lang');
    await expect(langPreview).toBeVisible();
    await expect(langPreview).toContainText('/lang de');
    await expect(langPreview).toContainText('German');

    // Refining a selection that carried a `/lang de` override produces a
    // polished German version of the user's message.
    await page.getByTestId('capture-refine').click();

    await expect(page.getByText('Der Q3-Releaseplan steht — keine Verzögerungen. Lieferung voraussichtlich am Montag.')).toBeVisible();

    const calls = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string }> }).__TAURI_MOCK_CALLS__,
    );
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);
  });
});
