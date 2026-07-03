import { test, expect } from '../fixtures/setup';

/**
 * S14: Built-in preset override warning — given the Presets screen, when the
 * user selects a built-in preset (e.g. /formal), edits it, and saves, then a
 * confirmation dialog (`preset-override-*`) warns that saving will shadow
 * the shipped default. Cancelling aborts (no `preset_save`, preset still
 * shows as a plain built-in). Confirming fires `preset_save` and the preset
 * now shows up flagged as overridden.
 * (design-redrafter.md, wireframes/presets.html, controls/presets.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Built-in preset override warning (S14)', () => {
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
          availableModels: ['claude-opus-4-6'],
          keyRef: '1',
        },
      ],
      activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
    },
  });

  test('S14: editing a built-in and saving warns before shadowing the shipped default; cancel aborts, confirm saves the override', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-presets').click();
    await expect(page.getByTestId('presets-screen')).toBeVisible();

    await page.getByTestId('preset-item-formal').click();
    await expect(page.getByTestId('preset-builtin-badge')).toBeVisible();
    // Not yet overridden -- no "overridden" chip on the row.
    await expect(page.getByTestId('preset-item-formal').getByText('overridden')).toHaveCount(0);

    await page
      .getByTestId('preset-direction')
      .fill('Rewrite formally and professionally; no slang or emoji; keep meaning. Extra polish.');

    // --- Save under the built-in trigger: warn before shadowing the default ---
    await page.getByTestId('preset-save').click();
    await expect(page.getByTestId('preset-override-confirm')).toBeVisible();
    await expect(page.getByTestId('preset-override-cancel')).toBeVisible();
    await expect(page.getByText('Modify built-in preset?')).toBeVisible();

    // --- Cancel: aborts, no preset_save, preset stays a plain built-in ---
    await page.getByTestId('preset-override-cancel').click();
    await expect(page.getByTestId('preset-override-confirm')).toHaveCount(0);
    expect((await mockCalls(page)).some((c) => c.cmd === 'preset_save')).toBe(false);
    await expect(page.getByTestId('preset-item-formal').getByText('overridden')).toHaveCount(0);

    // --- Retry and confirm: saves the override via preset_save ---
    await page.getByTestId('preset-save').click();
    await expect(page.getByTestId('preset-override-confirm')).toBeVisible();
    await page.getByTestId('preset-override-confirm').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'preset_save'))
      .toBe(true);
    const saveCall = (await mockCalls(page)).find((c) => c.cmd === 'preset_save');
    expect(saveCall!.args.trigger).toBe('formal');
    expect(saveCall!.args.direction).toBe(
      'Rewrite formally and professionally; no slang or emoji; keep meaning. Extra polish.',
    );

    await expect(page.getByTestId('preset-override-confirm')).toHaveCount(0);
    await expect(page.getByTestId('preset-item-formal').getByText('overridden')).toBeVisible();
  });
});
