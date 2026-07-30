import { test, expect } from '../fixtures/setup';

/**
 * S13: Preset create and save — given the Presets screen, when the user
 * clears the editor to a new preset, fills in a trigger and direction, and
 * saves it, then `preset_save` fires with the entered fields and the new
 * preset shows up in the list (in the "My presets" group, since it isn't
 * one of the shipped built-ins).
 * (design-redrafter.md, wireframes/presets.html, controls/presets.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Preset create and save (S13)', () => {
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

  test('S13: creating and saving a new preset calls preset_save and shows it in the list', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-presets').click();
    await expect(page.getByTestId('presets-screen')).toBeVisible();

    // The 5 shipped built-ins are there from the start; the new preset isn't yet.
    await expect(page.getByTestId('preset-item-formal')).toBeVisible();
    await expect(page.getByTestId('preset-item-standup')).toHaveCount(0);

    await page.getByTestId('preset-new').click();

    await page.getByTestId('preset-name').fill('/standup');
    await page
      .getByTestId('preset-direction')
      .fill('Reformat into a standup update: Yesterday / Today / Blockers.');

    await page.getByTestId('preset-save').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'preset_save'))
      .toBe(true);
    const saveCall = (await mockCalls(page)).find((c) => c.cmd === 'preset_save');
    expect(saveCall!.args.trigger).toBe('standup');
    expect(saveCall!.args.direction).toBe('Reformat into a standup update: Yesterday / Today / Blockers.');

    // No override confirmation should have fired — 'standup' isn't a built-in.
    expect((await mockCalls(page)).some((c) => c.cmd === 'preset_reset_default')).toBe(false);

    const newItem = page.getByTestId('preset-item-standup');
    await expect(newItem).toBeVisible();
  });
});
