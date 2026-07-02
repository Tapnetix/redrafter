import { test, expect } from '../fixtures/setup';

// S1: Hotkey capture and inject loop — the core default-refine path. Given
// text selected in another app and an active model, when the user presses
// the hotkey (opening the capture panel) and refines, then the refined
// result blind-injects in place of the selection and the original is saved
// for restore. (design-redrafter.md S1, wireframes/capture.html)
test.describe('Hotkey capture and inject loop', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineOutcome: {
        original: 'we good with the Q3 release plan i think, no delays i hope',
        refined: "We're good with the Q3 release plan — no delays.",
        model: 'claude-opus-4-6',
      },
    },
  });

  test('S1: hotkey capture refines and blind-injects the result, keeping the original for restore', async ({
    page,
  }) => {
    // Pressing the hotkey opens the capture panel over the captured
    // selection (the standalone /capture route stands in for the real
    // hotkey-triggered overlay window, wired by A14).
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();
    await expect(page.getByTestId('capture-editor')).toBeVisible();
    const refineBtn = page.getByTestId('capture-refine');
    await expect(refineBtn).toBeVisible();

    // Refine the captured selection — a single backend call performs
    // capture -> prompt -> model -> inject (blind inject: no review step).
    await refineBtn.click();

    // The refined result has replaced the selection: the Done state shows
    // the refined text and the restore control.
    await expect(page.getByText("We're good with the Q3 release plan — no delays.")).toBeVisible();
    const restoreBtn = page.getByTestId('capture-restore');
    await expect(restoreBtn).toBeVisible();

    const calls = await page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string }> }).__TAURI_MOCK_CALLS__);
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);
    // The blind inject happens as part of the single `refine` call on the
    // backend; the frontend never calls `inject_text` directly for it.
    expect(calls.some((c) => c.cmd === 'inject_text')).toBe(false);

    // The original is retained for restore: restoring re-injects it via the
    // dedicated restore commands.
    await restoreBtn.click();
    await expect(page.getByText('Restored original')).toBeVisible();

    const callsAfterRestore = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string; args: Record<string, unknown> }> }).__TAURI_MOCK_CALLS__,
    );
    expect(callsAfterRestore.some((c) => c.cmd === 'restore_original')).toBe(true);
    const injectCall = callsAfterRestore.find((c) => c.cmd === 'inject_text');
    expect(injectCall?.args?.text).toBe('we good with the Q3 release plan i think, no delays i hope');
  });
});
