import { test, expect } from '../fixtures/setup';

/**
 * S31: History detail view — given a populated History screen, when the
 * user opens a past entry's detail dialog, then it shows the entry's full
 * original/refined text, model, and time; closing it dismisses the dialog,
 * and restoring from it calls `history_restore` for that entry and closes
 * the dialog too. (design-redrafter.md, wireframes/history.html,
 * controls/history.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('History detail view (S31)', () => {
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
          id: '1',
          original:
            'we good with the Q3 release plan i think, no delays i hope, the engineers are working super hard and we should ship monday probably',
          refined:
            "We're good with the Q3 release plan — no delays. The engineers are working hard and we're on track to ship Monday.",
          model: 'claude-sonnet-4-6',
          createdAt: Date.now() - 2 * 60_000,
        },
      ],
    },
  });

  test('S31: opening a history entry shows its full original/refined/model/time', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await expect(page.getByTestId('history-row')).toBeVisible();

    await page.getByTestId('history-view').click();

    const detail = page.getByTestId('history-detail');
    await expect(detail).toBeVisible();
    await expect(page.getByTestId('history-detail-original')).toContainText(
      'the engineers are working super hard',
    );
    await expect(page.getByTestId('history-detail-refined')).toContainText(
      "we're on track to ship Monday",
    );
    await expect(detail).toContainText('claude-sonnet-4-6');
  });

  test('S31: closing the detail dialog dismisses it', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await page.getByTestId('history-view').click();
    await expect(page.getByTestId('history-detail')).toBeVisible();

    await page.getByTestId('history-detail-close').click();

    await expect(page.getByTestId('history-detail')).toBeHidden();
  });

  test('S31: restoring from the detail dialog calls history_restore and closes the dialog', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await page.getByTestId('history-view').click();
    await expect(page.getByTestId('history-detail')).toBeVisible();

    await page.getByTestId('history-detail-restore').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_restore' && c.args.id === '1'))
      .toBe(true);
    await expect(page.getByTestId('history-detail')).toBeHidden();
  });
});
