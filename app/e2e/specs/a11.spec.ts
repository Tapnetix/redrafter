import { test, expect } from '../fixtures/setup';

// S19: No-active-model routing — given no active model is selected, when
// the user invokes refine from the capture panel, then the backend `refine`
// call rejects with `no_active_model` and the panel routes the user to pick
// one (the no-active-model state with its `capture-no-model-cta` control),
// rather than failing silently. (design-redrafter.md S19, wireframes/capture.html,
// controls/capture.json)
test.describe('No-active-model routing', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineError: 'no_active_model',
    },
  });

  test('S19: refining with no active model shows the no-active-model state with a control to pick one', async ({
    page,
  }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();
    const refineBtn = page.getByTestId('capture-refine');
    await expect(refineBtn).toBeVisible();

    await refineBtn.click();

    // The mocked `refine` call rejects with `no_active_model`; the failure
    // is not silent — the panel routes to the no-active-model state.
    await expect(page.getByTestId('capture-no-model')).toBeVisible();
    await expect(page.getByText('Choose an active model before refining. Your text is untouched.')).toBeVisible();

    // The CTA that routes the user to the Models screen to pick one.
    const noModelCta = page.getByTestId('capture-no-model-cta');
    await expect(noModelCta).toBeVisible();
    await expect(noModelCta).toHaveText('Choose a model');

    const calls = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string }> }).__TAURI_MOCK_CALLS__,
    );
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);
    // The original selection is left untouched: no inject occurs on failure.
    expect(calls.some((c) => c.cmd === 'inject_text')).toBe(false);
  });
});
