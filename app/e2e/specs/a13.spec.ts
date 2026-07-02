import { test, expect } from '../fixtures/setup';

// S36: Runtime permission loss — given Accessibility is revoked at runtime,
// when the user invokes refine, then the capture panel shows a
// permission-needed state (instead of a refined result) and refine is
// blocked until the user re-grants the permission from System Settings.
// (design-redrafter.md S36, wireframes/capture.html, controls/capture.json)
test.describe('Runtime permission loss (S36)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineError: 'permission_denied',
    },
  });

  test('S36: revoked Accessibility permission shows a permission-needed state that blocks refine', async ({
    page,
  }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();
    const refineBtn = page.getByTestId('capture-refine');
    await expect(refineBtn).toBeVisible();

    // Invoking refine hits the backend, which rejects with
    // 'permission_denied' because Accessibility was revoked mid-session.
    await refineBtn.click();

    const calls = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string }> }).__TAURI_MOCK_CALLS__,
    );
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);

    // The panel shows the permission-needed state, not a refined result:
    // refine is blocked until the permission is re-granted.
    const permSection = page.getByTestId('capture-permission');
    await expect(permSection).toBeVisible();
    await expect(permSection).toContainText('Accessibility permission needed');
    await expect(page.getByTestId('capture-done')).not.toBeVisible();

    const openSettingsBtn = page.getByTestId('capture-perm-open-settings');
    await expect(openSettingsBtn).toBeVisible();

    // Clicking through to re-grant invokes permission_open_settings on the
    // backend (opens macOS System Settings for Accessibility).
    await openSettingsBtn.click();

    const callsAfterOpenSettings = await page.evaluate(
      () => (window as unknown as { __TAURI_MOCK_CALLS__: Array<{ cmd: string }> }).__TAURI_MOCK_CALLS__,
    );
    expect(callsAfterOpenSettings.some((c) => c.cmd === 'permission_open_settings')).toBe(true);
  });
});
