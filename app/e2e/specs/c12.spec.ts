import { test, expect } from '../fixtures/setup';

/**
 * S30: History copy — given a populated History screen, when the user
 * copies a past entry's refined text (either the per-row copy control or
 * the detail dialog's "Copy refined" button), then `history_copy` is called
 * for that entry and its refined text lands on the OS clipboard.
 * (design-redrafter.md, wireframes/history.html, controls/history.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('History copy (S30)', () => {
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
          original: 'we good with the release plan i think, shipping monday probably',
          refined: "We're good with the release plan — on track to ship Monday.",
          model: 'claude-opus-4-6',
          createdAt: Date.now() - 60_000,
        },
      ],
    },
  });

  test('S30: copying a history row calls history_copy and puts the refined text on the clipboard', async ({
    page,
    context,
  }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);

    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await expect(page.getByTestId('history-row')).toBeVisible();

    await page.getByTestId('history-copy').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_copy' && c.args.id === '1'))
      .toBe(true);

    await expect
      .poll(() => page.evaluate(() => navigator.clipboard.readText()))
      .toBe("We're good with the release plan — on track to ship Monday.");
  });

  test('S30: copying from the detail dialog calls history_copy and puts the refined text on the clipboard', async ({
    page,
    context,
  }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);

    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await page.getByTestId('history-view').click();
    await expect(page.getByTestId('history-detail')).toBeVisible();

    await page.getByTestId('history-detail-copy').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'history_copy' && c.args.id === '1'))
      .toBe(true);

    await expect
      .poll(() => page.evaluate(() => navigator.clipboard.readText()))
      .toBe("We're good with the release plan — on track to ship Monday.");
  });
});
