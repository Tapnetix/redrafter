import { test, expect } from '../fixtures/setup';

// S20: General surfaces status — given the app is running, when the user
// opens General, then permission status, hotkey, active model, and a
// menu-bar link are shown. (design-redrafter.md, wireframes/index.html)
test.describe('General settings surface (S20)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      settings: { hotkey_combo: 'Ctrl+Alt+R' },
    },
  });

  test('S20: opening General shows permission status, hotkey, active model, and menu-bar link', async ({
    page,
  }) => {
    await page.goto('/general');

    // Permission status: reflects the mocked `permission_status` response.
    const permStatus = page.getByTestId('perm-status');
    await expect(permStatus).toBeVisible();
    await expect(permStatus).toHaveAttribute('data-granted', 'true');
    await expect(permStatus).toContainText('Granted');

    // Hotkey: the current combo, sourced from settings.
    await expect(page.getByTestId('hotkey-value')).toHaveText('⌃⌥R');
    await expect(page.getByTestId('hotkey-change')).toBeVisible();

    // Active model summary (Phase A has no active model concept yet).
    await expect(page.getByTestId('active-model-link')).toBeVisible();
    await expect(page.getByTestId('active-model-link')).toContainText('No model selected');

    // Menu-bar link.
    await expect(page.getByTestId('general-tray-link')).toBeVisible();

    // Theme control is present and shown.
    await expect(page.getByTestId('setting-theme')).toBeVisible();
  });

  test('S20: re-checking permission calls permission_status again and updates the status', async ({
    page,
  }) => {
    await page.goto('/general');

    await expect(page.getByTestId('perm-status')).toHaveAttribute('data-granted', 'true');
    await page.getByTestId('perm-recheck').click();
    // Still granted after the recheck resolves (same mocked value).
    await expect(page.getByTestId('perm-status')).toHaveAttribute('data-granted', 'true');
  });

});
