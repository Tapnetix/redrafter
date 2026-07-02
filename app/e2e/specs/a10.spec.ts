import { test, expect } from '../fixtures/setup';

// S12: Restore original. Given a completed blind-inject, when the user
// triggers restore, then the pre-refine original is put back via
// `restore_original` + `inject_text`. (design-redrafter.md S12,
// wireframes/capture.html, controls/capture.json `capture-restore`)
test.describe('Restore original', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineOutcome: {
        original: 'the quarterly numbers looks good to me, lets ship it',
        refined: 'The quarterly numbers look good to me — let’s ship it.',
        model: 'claude-opus-4-6',
      },
    },
  });

  test('S12: restoring after a completed blind-inject puts the pre-refine original back', async ({ page }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();

    // Complete a blind-inject refine first, landing in the Done state.
    await page.getByTestId('capture-refine').click();
    await expect(page.getByText('The quarterly numbers look good to me — let’s ship it.')).toBeVisible();

    const restoreBtn = page.getByTestId('capture-restore');
    await expect(restoreBtn).toBeVisible();
    await restoreBtn.click();

    // The panel reflects that the original has been restored in place.
    await expect(page.getByText('Restored original')).toBeVisible();

    // Restore drives the two backend commands named in controls/capture.json
    // for `capture-restore`: `restore_original` (fetch the saved pre-refine
    // text) followed by `inject_text` (put it back in place of the refined
    // result), with the saved original as the injected payload.
    const calls = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string; args: Record<string, unknown> }> }).__TAURI_MOCK_CALLS__,
    );
    expect(calls.some((c) => c.cmd === 'restore_original')).toBe(true);
    const injectCall = calls.find((c) => c.cmd === 'inject_text');
    expect(injectCall?.args?.text).toBe('the quarterly numbers looks good to me, lets ship it');
  });
});
