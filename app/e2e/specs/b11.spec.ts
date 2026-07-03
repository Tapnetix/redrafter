import { test, expect } from '../fixtures/setup';

/**
 * S2: Inline direction + message — given a selection containing the inline
 * command syntax (`/rd` direction, `/m` message, `/q` quote, `/lang`
 * language override), when the capture panel is shown, then the live
 * command-parse preview (`capture-preview`, B6 over B4's `command_parser`)
 * reflects exactly what refine will do with each tag — before the user ever
 * clicks Refine. (design-redrafter.md S2, wireframes/capture.html,
 * controls/capture.json)
 *
 * The selection is injected via the `?text=` query param the Capture route
 * (app/src/app/capture/page.tsx) additively wires to Capture.tsx's existing
 * `capturedText` prop seam (B6) — no change to Capture.tsx or
 * command-preview.ts.
 */

const SELECTION =
  "/rd keep it warm but concise /m We're good with the release plan /q On Mon, Alex wrote: are we still on track? /lang de";

test.describe('Inline command parser preview (S2)', () => {
  test('S2: a selection with /rd, /m, /q, and /lang tags is parsed and reflected in the live preview before refine', async ({
    page,
  }) => {
    await page.goto(`/capture?text=${encodeURIComponent(SELECTION)}`);

    const editor = page.getByTestId('capture-editor');
    await expect(editor).toBeVisible();
    await expect(editor).toContainText(SELECTION);

    const preview = page.getByTestId('capture-preview');
    await expect(preview).toBeVisible();

    // /rd -> a direction was recognized (tag stripped from the parsed
    // breakdown, unlike the raw editor text above).
    await expect(preview).toContainText('direction');
    // /m -> the user's message was recognized.
    await expect(preview).toContainText('your message');
    // /q -> an explicit quote override was recognized.
    await expect(preview).toContainText('quote detected');
    // /lang de -> the target language code is reflected, not left at "auto".
    await expect(preview).toContainText('lang: de');
    await expect(preview).not.toContainText('lang: auto');

    // Refine hasn't been invoked yet -- this is purely a pre-refine preview.
    const mockCalls = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: { cmd: string }[] }).__TAURI_MOCK_CALLS__,
    );
    expect(mockCalls.some((c) => c.cmd === 'refine')).toBe(false);
  });
});
