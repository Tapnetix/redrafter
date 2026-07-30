import { test, expect } from '../fixtures/setup';

/**
 * S29: Preset export — given the Presets screen with user-saved presets,
 * when the user opens the Export dialog, `preset_export` fires and the
 * dialog shows the user presets as portable JSON; clicking "Copy to
 * clipboard" re-fetches the export and writes that same JSON to the
 * clipboard.
 * (wireframes/presets.html, controls/presets.json: preset-export,
 * preset-export-copy, preset-export-close)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

async function clipboardWrites(page: import('@playwright/test').Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __clipboardWrites__: string[] }).__clipboardWrites__ || []);
}

test.describe('Preset export (S29)', () => {
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
      presets: [
        {
          trigger: 'reply-de',
          direction: 'Reply to the quoted message in German. Keep it warm but concise.',
          lang: 'de',
          inject: 'review',
        },
        {
          trigger: 'standup',
          direction: 'Reformat into a standup update: Yesterday / Today / Blockers.',
        },
      ],
    },
  });

  test('S29: exporting presets fires preset_export and shows/copies the user presets as JSON', async ({ page }) => {
    // Stub the clipboard so "Copy to clipboard" is observable without OS
    // clipboard permissions in headless Chromium.
    await page.addInitScript(() => {
      (window as unknown as { __clipboardWrites__: string[] }).__clipboardWrites__ = [];
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          writeText: (text: string) => {
            (window as unknown as { __clipboardWrites__: string[] }).__clipboardWrites__.push(text);
            return Promise.resolve();
          },
        },
      });
    });

    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-presets').click();
    await expect(page.getByTestId('presets-screen')).toBeVisible();

    // Both seeded user presets show up in "My presets" before export.
    await expect(page.getByTestId('preset-item-reply-de')).toBeVisible();
    await expect(page.getByTestId('preset-item-standup')).toBeVisible();

    await page.getByTestId('preset-export').click();

    await expect(page.getByTestId('export-modal')).toBeVisible();
    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'preset_export').length)
      .toBeGreaterThanOrEqual(1);

    // The dialog shows the exported presets as portable JSON: an array
    // containing exactly the two user-saved presets, not the built-ins.
    // `preset_export` resolves asynchronously, so wait for the dialog to
    // populate before parsing it.
    await expect.poll(async () => (await page.getByTestId('preset-export-text').textContent()) || '').not.toBe('');
    const exportText = await page.getByTestId('preset-export-text').textContent();
    const exported = JSON.parse(exportText ?? '[]') as { trigger: string; direction: string }[];
    expect(exported).toHaveLength(2);
    const triggers = exported.map((p) => p.trigger).sort();
    expect(triggers).toEqual(['reply-de', 'standup']);
    const replyDe = exported.find((p) => p.trigger === 'reply-de');
    expect(replyDe!.direction).toBe('Reply to the quoted message in German. Keep it warm but concise.');

    // Copying re-fetches the export and writes the same JSON to the clipboard.
    await page.getByTestId('preset-export-copy').click();

    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'preset_export').length)
      .toBeGreaterThanOrEqual(2);
    await expect.poll(async () => (await clipboardWrites(page)).length).toBe(1);
    const [copied] = await clipboardWrites(page);
    expect(JSON.parse(copied)).toEqual(exported);

    await page.getByTestId('preset-export-close').click();
    await expect(page.getByTestId('export-modal')).toHaveCount(0);
  });
});
