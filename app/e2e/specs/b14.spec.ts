import { test, expect } from '../fixtures/setup';

/**
 * S7: Manual model-id fallback — given a provider/endpoint that doesn't
 * support model listing (connection_refresh_models resolves
 * ManualEntryRequired), when the user types a model id into the manual
 * fallback control and submits, then model_add_manual is called and the
 * model becomes available and enabled on the connection. (design-redrafter.md,
 * wireframes/connections.html, controls/connections.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Manual model-id fallback (S7)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [],
      // No discoverModels seeded, and connection_refresh_models rejects with
      // this reason -- mirrors DiscoveryResult::ManualEntryRequired for a
      // provider/endpoint with no list-models support.
      manualEntryRequired: 'endpoint has no /v1/models support',
    },
  });

  test('S7: manually entering a model id when discovery is unavailable makes the model available and enabled', async ({
    page,
  }) => {
    await page.goto('/connections');

    await expect(page.getByTestId('connections-empty')).toBeVisible();
    await page.getByTestId('connections-empty-cta').click();

    const modal = page.getByTestId('connection-modal');
    await expect(modal).toBeVisible();

    // Ollama needs no API key, so Test + Save exercise the fallback without
    // extra unrelated setup.
    await page.getByTestId('conn-provider-type').selectOption('ollama');

    await page.getByTestId('conn-test').click();
    await expect(page.getByTestId('conn-test-ok')).toBeVisible();

    await page.getByTestId('connection-save').click();

    // Save calls connection_add, then auto-discovers via
    // connection_refresh_models, which rejects (manual entry required).
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_add'))
      .toBe(true);
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_refresh_models'))
      .toBe(true);

    // No models were discovered, so the screen surfaces the manual fallback
    // with the reason from the backend, and the manual entry control is
    // present and usable.
    const discoveredList = page.getByTestId('conn-discovered-list');
    await expect(discoveredList).toBeVisible();
    await expect(discoveredList).toContainText('No models could be listed automatically');
    await expect(discoveredList).toContainText('endpoint has no /v1/models support');

    const manualInput = page.getByTestId('conn-add-model-manual');
    const manualAdd = page.getByTestId('conn-add-model-manual-add');
    await expect(manualInput).toBeVisible();
    await expect(manualAdd).toBeVisible();

    await manualInput.fill('llama3-8b-instruct');
    await manualAdd.click();

    // model_add_manual is invoked with the connection id and typed model id.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'model_add_manual'))
      .toBe(true);
    const manualCall = (await mockCalls(page)).find((c) => c.cmd === 'model_add_manual');
    expect(manualCall!.args.modelId).toBe('llama3-8b-instruct');

    // The manually-added model is now listed and enabled by default.
    const modelCheck = page.getByTestId('conn-model-check-llama3-8b-instruct');
    await expect(modelCheck).toBeVisible();
    await expect(modelCheck).toBeChecked();

    // The manual-entry text field is cleared after a successful add.
    await expect(manualInput).toHaveValue('');

    // Closing the sheet shows the connection with its newly-usable model.
    await page.getByTestId('connection-cancel').click();
    await expect(page.getByTestId('connections-list')).toBeVisible();
    await expect(page.getByTestId('connection-row-ollama')).toBeVisible();
    await expect(page.getByTestId('connection-row-ollama')).toContainText('1 models');
  });
});
