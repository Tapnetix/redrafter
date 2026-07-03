import { test, expect } from '../fixtures/setup';

/**
 * S23: Connection test error surfacing — given a connection that is
 * unreachable or has a bad key, when the user runs the connection test,
 * then the error is surfaced clearly in the UI (not swallowed), and the
 * connection is not silently treated as ok/saved. (design-redrafter.md,
 * wireframes/connections.html, controls/connections.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Connection test error surfacing (S23)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [],
      testConnectionError: 'unauthorized: invalid API key',
    },
  });

  test('S23: a failing connection test surfaces the error and is not silently saved as ok', async ({ page }) => {
    await page.goto('/connections');

    await expect(page.getByTestId('connections-empty')).toBeVisible();
    await page.getByTestId('connections-empty-cta').click();

    const modal = page.getByTestId('connection-modal');
    await expect(modal).toBeVisible();

    // Anthropic is the default provider type; fill in a bad API key.
    await expect(page.getByTestId('conn-provider-type')).toHaveValue('anthropic');
    await page.getByTestId('conn-api-key').fill('sk-ant-bad-key');

    await page.getByTestId('conn-test').click();

    // The error is surfaced clearly — not a silent failure.
    const errorChip = page.getByTestId('conn-test-error');
    await expect(errorChip).toBeVisible();
    await expect(errorChip).toContainText('unauthorized: invalid API key');

    // It must never be shown (even momentarily settled) as ok.
    await expect(page.getByTestId('conn-test-ok')).toHaveCount(0);

    const testCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_test');
    expect(testCall).toBeTruthy();
    expect(testCall!.args.providerKind).toBe('anthropic');
    expect(testCall!.args.apiKey).toBe('sk-ant-bad-key');

    // Closing without saving: the failed connection must not have been
    // silently persisted as if the test had succeeded.
    await page.getByTestId('connection-cancel').click();
    await expect(page.getByTestId('connections-empty')).toBeVisible();
    expect((await mockCalls(page)).some((c) => c.cmd === 'connection_add')).toBe(false);
  });
});
