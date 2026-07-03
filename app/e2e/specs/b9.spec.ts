import { test, expect } from '../fixtures/setup';

/**
 * S9: Tray active-model switcher — given several enabled models (some
 * favorited), when the user opens the menu-bar tray, then the full model
 * list is collapsed by default (only the current active model shows); when
 * they expand the "Active model" row, favorites surface at the top and the
 * full per-connection list beneath; and picking a model calls
 * `tray_set_active_model` and updates the active indicator everywhere.
 * (design-redrafter.md, wireframes/tray.html, controls/tray.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Tray active-model switcher (S9)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [
        {
          id: '1',
          providerKind: 'anthropic',
          baseUrl: 'https://api.anthropic.com',
          enabledModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
          keyRef: '1',
        },
        {
          id: '2',
          providerKind: 'ollama',
          baseUrl: 'http://localhost:11434',
          enabledModels: ['qwen3:8b', 'llama3.1:8b'],
        },
      ],
      activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
      favoriteModels: [
        { connectionId: '1', modelId: 'claude-opus-4-6' },
        { connectionId: '1', modelId: 'claude-sonnet-4-6' },
        { connectionId: '2', modelId: 'qwen3:8b' },
      ],
    },
  });

  test('S9: switching the active model from the tray updates the active indicator', async ({ page }) => {
    await page.goto('/tray');

    // Collapsed by default: the active model shows in the row, but neither
    // the favorites nor the full per-connection list is rendered yet.
    const activeRow = page.getByTestId('tray-active-model');
    await expect(activeRow).toBeVisible();
    await expect(activeRow).toHaveAttribute('aria-expanded', 'false');
    await expect(activeRow).toContainText('claude-opus-4-6');
    await expect(page.getByTestId('tray-fav-claude-sonnet-4-6')).toHaveCount(0);
    await expect(page.getByTestId('tray-model-llama3.1:8b')).toHaveCount(0);

    // Expanding reveals favorites at the top, then the full grouped list.
    await activeRow.click();
    await expect(activeRow).toHaveAttribute('aria-expanded', 'true');

    const favSonnet = page.getByTestId('tray-fav-claude-sonnet-4-6');
    const favQwen = page.getByTestId('tray-fav-qwen3:8b');
    await expect(favSonnet).toBeVisible();
    await expect(favQwen).toBeVisible();
    // llama3.1:8b isn't favorited, so it only appears in the full list.
    await expect(page.getByTestId('tray-fav-llama3.1:8b')).toHaveCount(0);
    await expect(page.getByTestId('tray-model-llama3.1:8b')).toBeVisible();
    await expect(page.getByTestId('tray-model-claude-opus-4-6')).toBeVisible();

    // The active model's favorite entry is checked; a different one isn't.
    await expect(page.getByTestId('tray-fav-claude-opus-4-6')).toHaveAttribute('aria-checked', 'true');
    await expect(favSonnet).toHaveAttribute('aria-checked', 'false');

    // Picking a favorite sets it active via tray_set_active_model.
    await favSonnet.click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'tray_set_active_model'))
      .toBe(true);
    const call = (await mockCalls(page)).find((c) => c.cmd === 'tray_set_active_model');
    expect(call!.args.connectionId).toBe('1');
    expect(call!.args.modelId).toBe('claude-sonnet-4-6');

    // The active indicator updates everywhere the switcher shows a label.
    await expect(activeRow).toContainText('claude-sonnet-4-6');

    // Picking from the tray also collapses the switcher back down.
    await expect(activeRow).toHaveAttribute('aria-expanded', 'false');

    // Re-expand and confirm the full list also reflects the new active pick.
    await activeRow.click();
    await expect(page.getByTestId('tray-model-claude-sonnet-4-6')).toHaveAttribute('aria-checked', 'true');
    await expect(page.getByTestId('tray-model-claude-opus-4-6')).toHaveAttribute('aria-checked', 'false');
  });
});
