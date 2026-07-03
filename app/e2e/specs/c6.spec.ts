import { test, expect } from '../fixtures/setup';

/**
 * S34: Hotkey rebind and conflict — given the rebind dialog (General
 * screen), when the user captures a combo already in use, then a conflict
 * warning appears and the previous hotkey keeps working (per `hotkey.rs`'s
 * `apply_combo`: a conflict never touches the previously-registered
 * combo); when the user captures an unused combo instead, it saves via
 * `hotkey_set` and persists as the new hotkey. (design-redrafter.md,
 * wireframes/index.html, controls/index.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Hotkey rebind and conflict (S34)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      settings: { hotkey_combo: 'Ctrl+Alt+R' },
      // "Ctrl+Alt+T" simulates a combo already claimed elsewhere.
      hotkeyConflictCombo: 'Ctrl+Alt+T',
    },
  });

  test('S34: rebinding to a free combo saves via hotkey_set and persists', async ({ page }) => {
    await page.goto('/general');
    await expect(page.getByTestId('hotkey-value')).toHaveText('⌃⌥R');

    await page.getByTestId('hotkey-change').click();
    await expect(page.getByTestId('hotkey-modal')).toBeVisible();

    await page.getByTestId('hotkey-capture').focus();
    await page.keyboard.press('Control+Alt+S');
    await expect(page.getByTestId('hotkey-capture')).toContainText('⌃⌥S');

    await page.getByTestId('hotkey-save').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'hotkey_set' && c.args.combo === 'Ctrl+Alt+S'))
      .toBe(true);

    // Success: the dialog closes and the shown hotkey updates.
    await expect(page.getByTestId('hotkey-modal')).toBeHidden();
    await expect(page.getByTestId('hotkey-value')).toHaveText('⌃⌥S');

    // Persists: mirrors `hotkey.rs`'s `persist_result` writing the combo to
    // the settings store on success -- re-reading `hotkey_combo` (as a
    // fresh app launch's `register_startup` would) reflects the rebind,
    // not the original combo.
    const persisted = await page.evaluate(() =>
      (
        window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> } }
      ).__TAURI_INTERNALS__.invoke('settings_get', { key: 'hotkey_combo' }),
    );
    expect(persisted).toBe('Ctrl+Alt+S');
  });

  test('S34: rebinding to a conflicting combo shows a conflict warning and keeps the old hotkey', async ({
    page,
  }) => {
    await page.goto('/general');
    await expect(page.getByTestId('hotkey-value')).toHaveText('⌃⌥R');

    await page.getByTestId('hotkey-change').click();
    await expect(page.getByTestId('hotkey-modal')).toBeVisible();

    await page.getByTestId('hotkey-capture').focus();
    await page.keyboard.press('Control+Alt+T');
    await expect(page.getByTestId('hotkey-capture')).toContainText('⌃⌥T');

    await page.getByTestId('hotkey-save').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'hotkey_set' && c.args.combo === 'Ctrl+Alt+T'))
      .toBe(true);

    // Conflict: a warning appears, the dialog stays open, and the hotkey
    // shown behind it (once dismissed) is unchanged.
    await expect(page.getByTestId('hotkey-conflict')).toBeVisible();
    await expect(page.getByTestId('hotkey-modal')).toBeVisible();

    await page.getByTestId('hotkey-cancel').click();
    await expect(page.getByTestId('hotkey-modal')).toBeHidden();
    await expect(page.getByTestId('hotkey-value')).toHaveText('⌃⌥R');
  });
});
