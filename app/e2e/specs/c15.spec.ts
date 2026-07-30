import { test, expect } from '../fixtures/setup';

/**
 * S33: History clear-all — given a populated History screen, when the user
 * clicks "Clear history", then a confirmation dialog opens without clearing
 * anything yet; cancelling dismisses it and leaves the history untouched,
 * while confirming calls `history_clear` and empties the list (showing the
 * empty state). (design-redrafter.md, wireframes/history.html,
 * controls/history.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('History clear-all (S33)', () => {
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
    },
  });

  test('S33: clicking Clear history opens a confirmation without clearing yet', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-history').click();
    await expect(page.getByTestId('history-row')).toHaveCount(2);

    await page.getByTestId('history-clear').click();

    await expect(page.getByTestId('history-clear-modal')).toBeVisible();
    expect((await mockCalls(page)).some((c) => c.cmd === 'history_clear')).toBe(false);
    await expect(page.getByTestId('history-row')).toHaveCount(2);
  });

  test('S33: cancelling the confirmation leaves the history untouched', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-history').click();
    await page.getByTestId('history-clear').click();
    await expect(page.getByTestId('history-clear-modal')).toBeVisible();

    await page.getByTestId('history-clear-cancel').click();

    await expect(page.getByTestId('history-clear-modal')).toBeHidden();
    await expect(page.getByTestId('history-row')).toHaveCount(2);
  });

  test('S33: confirming clears history via history_clear and shows the empty state', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-history').click();
    await page.getByTestId('history-clear').click();
    await expect(page.getByTestId('history-clear-modal')).toBeVisible();

    await page.getByTestId('history-clear-confirm').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_clear'))
      .toBe(true);
    await expect(page.getByTestId('history-empty')).toBeVisible();
    await expect(page.getByTestId('history-row')).toHaveCount(0);
  });
});
