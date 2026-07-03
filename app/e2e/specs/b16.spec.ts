import { test, expect } from '../fixtures/setup';

/**
 * S11: Review-and-confirm inject — given inject mode is review-and-confirm,
 * when a refine completes, then the user sees the result and can accept,
 * edit, or discard before it replaces the selection.
 * (design-redrafter.md S11, wireframes/capture.html, controls/capture.json
 * `capture-review`/`capture-accept`/`capture-edit`/`capture-edit-field`/
 * `capture-edit-accept`/`capture-discard`)
 *
 * B5 (orchestrator review branch) + B6 (capture-review state machine) build
 * the surface this spec exercises; the mock's `refineOutcome.status:
 * 'pending_review'` (RefineFixture, B6) is the seam that puts `refine` into
 * the review branch instead of a blind inject.
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Review-and-confirm inject (S11)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineOutcome: {
        original: 'the quarterly numbers looks good to me, lets ship it',
        refined: 'The quarterly numbers look good to me — let’s ship it.',
        model: 'claude-opus-4-6',
        status: 'pending_review',
      },
    },
  });

  test('S11: accepting a review-and-confirm refine injects the refined text', async ({ page }) => {
    await page.goto('/capture');
    await expect(page.getByTestId('capture-panel')).toBeVisible();

    await page.getByTestId('capture-refine').click();

    // The refine result is suspended for review rather than already
    // injected — no inject_text yet, and the review surface shows both the
    // original and refined text with Accept/Edit/Discard.
    const review = page.getByTestId('capture-review');
    await expect(review).toBeVisible();
    await expect(review).toContainText('the quarterly numbers looks good to me, lets ship it');
    await expect(review).toContainText('The quarterly numbers look good to me — let’s ship it.');
    await expect(page.getByTestId('capture-accept')).toBeVisible();
    await expect(page.getByTestId('capture-edit')).toBeVisible();
    await expect(page.getByTestId('capture-discard')).toBeVisible();
    expect((await mockCalls(page)).some((c) => c.cmd === 'inject_text')).toBe(false);

    await page.getByTestId('capture-accept').click();

    // Accept pastes the refined (unedited) text via inject_text.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'inject_text'))
      .toBe(true);
    const injectCall = (await mockCalls(page)).find((c) => c.cmd === 'inject_text');
    expect(injectCall?.args?.text).toBe('The quarterly numbers look good to me — let’s ship it.');
    await expect(page.getByTestId('capture-done')).toBeVisible();
  });

  test('S11: editing before accepting injects the edited text, not the original refine', async ({ page }) => {
    await page.goto('/capture');
    await expect(page.getByTestId('capture-panel')).toBeVisible();

    await page.getByTestId('capture-refine').click();
    await expect(page.getByTestId('capture-review')).toBeVisible();

    await page.getByTestId('capture-edit').click();
    const editField = page.getByTestId('capture-edit-field');
    await expect(editField).toBeVisible();
    await expect(editField).toHaveValue('The quarterly numbers look good to me — let’s ship it.');

    await editField.fill('The quarterly numbers look great — ship it now.');
    await page.getByTestId('capture-edit-accept').click();

    // Accept-after-edit pastes the user-edited text, not the original
    // refine result.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'inject_text'))
      .toBe(true);
    const injectCall = (await mockCalls(page)).find((c) => c.cmd === 'inject_text');
    expect(injectCall?.args?.text).toBe('The quarterly numbers look great — ship it now.');
    await expect(page.getByTestId('capture-done')).toBeVisible();
  });

  test('S11: discarding the reviewed result injects nothing and leaves the original untouched', async ({ page }) => {
    await page.goto('/capture');
    await expect(page.getByTestId('capture-panel')).toBeVisible();

    await page.getByTestId('capture-refine').click();
    await expect(page.getByTestId('capture-review')).toBeVisible();

    await page.getByTestId('capture-discard').click();

    // Discard calls cancel_refine and never inject_text — nothing was
    // pasted, so the original selection is untouched.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'cancel_refine'))
      .toBe(true);
    expect((await mockCalls(page)).some((c) => c.cmd === 'inject_text')).toBe(false);

    // Back at the input state, ready to refine again rather than stuck in review.
    await expect(page.getByTestId('capture-review')).toHaveCount(0);
    await expect(page.getByTestId('capture-refine')).toBeVisible();
  });
});
