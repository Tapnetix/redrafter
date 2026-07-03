import { test, expect } from '../fixtures/setup';

/**
 * S37: Tray check-for-updates — given the menu-bar tray, when the user
 * clicks "Check for updates…" (`tray-updates`), then `tray_check_updates`
 * fires, the control shows a checking state while the call is in flight,
 * and then reflects the outcome: either "up to date" (no update available)
 * or an "update available" state naming the new version.
 * (wireframes/tray.html, controls/tray.json: `tray-updates`)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

const CONNECTIONS_TESTDATA = {
  appName: 'redrafter',
  permissionGranted: true,
  connections: [
    {
      id: '1',
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      enabledModels: ['claude-opus-4-6'],
      keyRef: '1',
    },
  ],
  activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
};

test.describe('Tray check-for-updates (S37), already up to date', () => {
  test.use({
    testData: {
      ...CONNECTIONS_TESTDATA,
      updateCheckResult: { updateAvailable: false },
    },
  });

  test('S37: checking for updates surfaces the up-to-date result', async ({ page }) => {
    await page.goto('/tray');

    const updates = page.getByTestId('tray-updates');
    await expect(updates).toBeVisible();
    await expect(updates).toContainText('Check for updates');

    await updates.click();

    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_check_updates').length)
      .toBe(1);

    await expect(page.getByTestId('tray-updates-uptodate')).toBeVisible();
    await expect(page.getByTestId('tray-updates-available')).toHaveCount(0);
    await expect(page.getByTestId('tray-updates-checking')).toHaveCount(0);
  });
});

test.describe('Tray check-for-updates (S37), update available', () => {
  test.use({
    testData: {
      ...CONNECTIONS_TESTDATA,
      updateCheckResult: { updateAvailable: true, version: '1.4.0' },
    },
  });

  test('S37: checking for updates surfaces the available version', async ({ page }) => {
    await page.goto('/tray');

    await page.getByTestId('tray-updates').click();

    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_check_updates').length)
      .toBe(1);

    const available = page.getByTestId('tray-updates-available');
    await expect(available).toBeVisible();
    await expect(available).toContainText('1.4.0');
    await expect(page.getByTestId('tray-updates-uptodate')).toHaveCount(0);
    await expect(page.getByTestId('tray-updates-checking')).toHaveCount(0);
  });
});
