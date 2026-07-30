import { test, expect } from '../fixtures/setup';

/**
 * S15: History restore and re-refine — given a populated History screen,
 * when the user restores a past entry's original text, then `history_restore`
 * is called for that entry and its original is put back; when the user
 * re-refines a past entry instead, then `history_rerefine` is called and a
 * new result (with its own refined text) appears at the top of the list.
 * (design-redrafter.md, wireframes/history.html, controls/history.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('History restore and re-refine (S15)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [
        {
          id: '1',
          providerKind: 'anthropic',
          baseUrl: 'https://api.anthropic.com',
          enabledModels: ['claude-opus-4-6'],
        },
      ],
      historyEntries: [
        {
          id: '2',
          original: 'thanks, sounds good, will check the numbers tomorrow',
          refined: "Thanks — sounds good. I'll check the numbers tomorrow.",
          model: 'claude-opus-4-6',
          createdAt: Date.now() - 60_000,
        },
        {
          id: '1',
          original: 'we good with the release plan i think, shipping monday probably',
          refined: "We're good with the release plan — on track to ship Monday.",
          model: 'claude-opus-4-6',
          createdAt: Date.now() - 120_000,
        },
      ],
      historyRerefineRefined: 'A brand new, re-refined version of the original.',
    },
  });

  test('S15: restoring a past history entry restores its original text', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-history').click();
    await expect(page.getByTestId('history-list')).toBeVisible();

    const rows = page.getByTestId('history-row');
    await expect(rows).toHaveCount(2);

    // The most recently created entry (id 2) is listed first.
    await expect(rows.first()).toContainText('will check the numbers tomorrow');

    await rows.first().getByTestId('history-restore').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_restore' && c.args.id === '2'))
      .toBe(true);
  });

  test('S15: re-refining a past history entry produces a new result at the top of the list', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-history').click();
    const rows = page.getByTestId('history-row');
    await expect(rows).toHaveCount(2);

    // Re-refine the older entry (id 1).
    await rows.last().getByTestId('history-rerefine').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_rerefine' && c.args.id === '1'))
      .toBe(true);

    // A brand new entry shows up at the top of the list with the new result.
    await expect(rows).toHaveCount(3);
    await expect(rows.first()).toContainText('A brand new, re-refined version of the original.');
  });
});
