import { test, expect } from '../fixtures/setup';

/**
 * S24: Remove connection — given a connected provider, when the user opens
 * it for editing and removes it, then confirms, its connection is deleted
 * (via `connection_remove`) and it disappears from the Connections list.
 * (design-redrafter.md, wireframes/connections.html, controls/connections.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Remove connection (S24)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [
        {
          id: 'conn-ollama-1',
          providerKind: 'ollama',
          baseUrl: 'http://localhost:11434',
          enabledModels: ['llama3'],
          availableModels: ['llama3'],
          keyRef: null,
        },
      ],
    },
  });

  test('S24: removing a connection prompts for confirmation, then deletes it', async ({ page }) => {
    await page.goto('/connections');

    await expect(page.getByTestId('connections-list')).toBeVisible();
    await expect(page.getByTestId('connection-row-ollama')).toBeVisible();

    // Open the connection for editing to reach its Remove control.
    await page.getByTestId('connection-edit-ollama').click();
    const modal = page.getByTestId('connection-modal');
    await expect(modal).toBeVisible();

    await page.getByTestId('connection-remove').click();

    // Confirmation dialog appears; connection_remove has not fired yet.
    const confirmModal = page.getByTestId('connection-remove-modal');
    await expect(confirmModal).toBeVisible();
    expect((await mockCalls(page)).some((c) => c.cmd === 'connection_remove')).toBe(false);

    await page.getByTestId('connection-remove-confirm').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_remove'))
      .toBe(true);
    const removeCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_remove');
    expect(removeCall!.args.id).toBe('conn-ollama-1');

    // The confirmation dialog closes and the connection is gone from the
    // list — with no other connections seeded, the empty state shows.
    await expect(confirmModal).not.toBeVisible();
    await expect(page.getByTestId('connection-row-ollama')).not.toBeVisible();
    await expect(page.getByTestId('connections-empty')).toBeVisible();
  });
});
