import { test, expect } from '../fixtures/setup';

/**
 * The review panel (`app/src-tauri/src/review.rs`, `app/src/app/review/page.tsx`).
 *
 * Behavior's "Review & confirm" inject mode parks the refined draft instead of
 * injecting it. Every backend piece of that existed — the orchestrator's
 * PendingReview flow, inject_text (accept) and cancel_refine (discard) — but
 * no window ever showed it, so choosing the mode called the model and silently
 * dropped the answer. These drive the real panel page in a browser.
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

const PENDING = {
  original: 'i beleive this feature dont work correct',
  refined: "I believe this feature doesn't work correctly.",
  model: 'claude-opus-5',
  status: 'pending_review' as const,
};

test.describe('Review panel', () => {
  test.use({ testData: { appName: 'redrafter', permissionGranted: true, pendingReview: PENDING } });

  test('presents the original, the refined draft and the model', async ({ page }) => {
    await page.goto('/review');

    await expect(page.getByTestId('review-original')).toContainText('i beleive this feature dont work correct');
    await expect(page.getByTestId('review-refined')).toContainText("I believe this feature doesn't work correctly.");
    await expect(page.getByTestId('review-model')).toContainText('claude-opus-5');
  });

  test('inserting goes through review_accept, which restores focus first', async ({ page }) => {
    // Never inject_text directly from this window: the panel holds focus while
    // the user reads, so injecting from here would paste into the panel.
    await page.goto('/review');
    await page.getByTestId('review-accept').click();

    await expect.poll(async () => (await mockCalls(page)).map((c) => c.cmd)).toContain('review_accept');
    const accept = (await mockCalls(page)).find((c) => c.cmd === 'review_accept');
    expect(accept?.args.text).toBe("I believe this feature doesn't work correctly.");
    expect((await mockCalls(page)).map((c) => c.cmd)).not.toContain('inject_text');
  });

  test('an edited draft is what gets inserted', async ({ page }) => {
    await page.goto('/review');
    await page.getByTestId('review-edit').click();
    await page.getByTestId('review-edit-field').fill('My own wording, thanks.');
    await page.getByTestId('review-accept').click();

    await expect
      .poll(async () => (await mockCalls(page)).find((c) => c.cmd === 'review_accept')?.args.text)
      .toBe('My own wording, thanks.');
  });

  test('discarding inserts nothing', async ({ page }) => {
    await page.goto('/review');
    await page.getByTestId('review-discard').click();

    await expect.poll(async () => (await mockCalls(page)).map((c) => c.cmd)).toContain('review_discard');
    expect((await mockCalls(page)).map((c) => c.cmd)).not.toContain('review_accept');
  });

  // Split rather than reloading mid-test: a reload races the mock's call log
  // and the panel's key listener, which made this flaky under parallel load.
  test('Escape discards', async ({ page }) => {
    await page.goto('/review');
    await page.getByTestId('review-refined').waitFor();

    await page.keyboard.press('Escape');

    await expect.poll(async () => (await mockCalls(page)).map((c) => c.cmd)).toContain('review_discard');
  });

  test('Cmd+Enter inserts', async ({ page }) => {
    await page.goto('/review');
    await page.getByTestId('review-refined').waitFor();

    await page.keyboard.press('ControlOrMeta+Enter');

    await expect.poll(async () => (await mockCalls(page)).map((c) => c.cmd)).toContain('review_accept');
  });
});

test.describe('Review panel with nothing pending', () => {
  test.use({ testData: { appName: 'redrafter', permissionGranted: true, pendingReview: null } });

  test('shows an empty state rather than an error', async ({ page }) => {
    await page.goto('/review');

    await expect(page.getByTestId('review-empty')).toBeVisible();
    await expect(page.getByTestId('review-accept')).toBeDisabled();
  });
});
