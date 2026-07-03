import { test, expect } from '../fixtures/setup';

/**
 * S6: Add connection discovers models — given valid provider credentials,
 * when the user tests/saves a connection, then its models are listed and a
 * default is enabled. (design-redrafter.md, wireframes/connections.html,
 * controls/connections.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Connection add + model discovery (S6)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [],
      discoverModels: ['claude-opus-4-6', 'claude-sonnet-4-6', 'claude-3-5-haiku'],
    },
  });

  test('S6: testing and saving a connection with valid credentials discovers models and enables a default', async ({
    page,
  }) => {
    await page.goto('/connections');

    await expect(page.getByTestId('connections-empty')).toBeVisible();
    await page.getByTestId('connections-empty-cta').click();

    const modal = page.getByTestId('connection-modal');
    await expect(modal).toBeVisible();

    // Anthropic is the default provider type; fill in an API key.
    await expect(page.getByTestId('conn-provider-type')).toHaveValue('anthropic');
    await page.getByTestId('conn-api-key').fill('sk-ant-test-key');

    await page.getByTestId('conn-test').click();
    await expect(page.getByTestId('conn-test-ok')).toBeVisible();

    const testCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_test');
    expect(testCall).toBeTruthy();
    expect(testCall!.args.providerKind).toBe('anthropic');
    expect(testCall!.args.apiKey).toBe('sk-ant-test-key');

    await page.getByTestId('connection-save').click();

    // Save calls connection_add, then auto-discovers via
    // connection_refresh_models.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_add'))
      .toBe(true);
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_refresh_models'))
      .toBe(true);

    const addCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_add');
    expect(addCall!.args.providerKind).toBe('anthropic');
    expect(addCall!.args.apiKey).toBe('sk-ant-test-key');

    // The discovered models are listed, with the default (first) one
    // enabled and the rest not.
    await expect(page.getByTestId('conn-discovered-list')).toBeVisible();
    const opusCheck = page.getByTestId('conn-model-check-claude-opus-4-6');
    const sonnetCheck = page.getByTestId('conn-model-check-claude-sonnet-4-6');
    const haikuCheck = page.getByTestId('conn-model-check-claude-3-5-haiku');
    await expect(opusCheck).toBeVisible();
    await expect(sonnetCheck).toBeVisible();
    await expect(haikuCheck).toBeVisible();
    await expect(opusCheck).toBeChecked();
    await expect(sonnetCheck).not.toBeChecked();
    await expect(haikuCheck).not.toBeChecked();

    // Closing the sheet shows the new connection in the populated list.
    await page.getByTestId('connection-cancel').click();
    await expect(page.getByTestId('connections-list')).toBeVisible();
    await expect(page.getByTestId('connection-row-anthropic')).toBeVisible();
  });
});
