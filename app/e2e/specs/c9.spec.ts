import { test, expect } from '../fixtures/setup';

/**
 * S28: Preset import — given the Presets screen, when the user opens the
 * Import dialog, pastes a JSON array of presets, and confirms, then
 * `preset_import` fires with the pasted JSON; a brand-new trigger in the
 * import shows up in "My presets", an entry whose trigger matches an
 * existing user preset is merged (its direction is overwritten) and flagged
 * as a conflict, and the dialog's result summary reflects both outcomes.
 * (wireframes/presets.html: import-modal / preset-import-*, controls/presets.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

const IMPORT_JSON = JSON.stringify([
  {
    trigger: 'standup',
    direction: 'Reformat into a standup update: Yesterday / Today / Blockers.',
    examples: [],
  },
  {
    trigger: 'exec-summary',
    direction: 'Overwritten by import: 3-bullet TL;DR, always lead with the ask.',
    examples: [],
  },
]);

test.describe('Preset import (S28)', () => {
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
      // A pre-existing user preset whose trigger collides with one of the
      // imported entries below, so the import is exercised as a merge with
      // a flagged conflict rather than only fresh additions.
      presets: [
        {
          trigger: 'exec-summary',
          direction: 'Summarize as a 3-bullet TL;DR for leadership. Lead with the decision or ask.',
        },
      ],
    },
  });

  test('S28: importing a JSON array of presets merges them and flags conflicts', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-presets').click();
    await expect(page.getByTestId('presets-screen')).toBeVisible();

    // Neither the brand-new trigger nor an updated exec-summary is present yet.
    await expect(page.getByTestId('preset-item-standup')).toHaveCount(0);
    await expect(page.getByTestId('preset-item-exec-summary')).toBeVisible();

    await page.getByTestId('preset-import').click();
    await expect(page.getByTestId('import-modal')).toBeVisible();

    await page.getByTestId('preset-import-paste').fill(IMPORT_JSON);
    await page.getByTestId('preset-import-confirm').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'preset_import'))
      .toBe(true);

    const importCall = (await mockCalls(page)).find((c) => c.cmd === 'preset_import');
    expect(importCall!.args.json).toBe(IMPORT_JSON);

    // The dialog's result summary reflects one fresh import and one conflict.
    const result = page.getByTestId('preset-import-result');
    await expect(result).toBeVisible();
    await expect(result).toContainText('Imported 2');
    await expect(result).toContainText('1 conflicts');

    await page.getByTestId('preset-import-cancel').click();
    await expect(page.getByTestId('import-modal')).toHaveCount(0);

    // The brand-new preset now appears in "My presets".
    await expect(page.getByTestId('preset-item-standup')).toBeVisible();

    // The existing preset was merged (overwritten), not duplicated.
    await expect(page.getByTestId('preset-item-exec-summary')).toBeVisible();
    await page.getByTestId('preset-item-exec-summary').click();
    await expect(page.getByTestId('preset-direction')).toHaveValue(
      'Overwritten by import: 3-bullet TL;DR, always lead with the ask.',
    );
  });
});
