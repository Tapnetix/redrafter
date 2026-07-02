import { test, expect } from '../fixtures/setup';

// S17: First-run permission gate. Given a fresh install, the onboarding
// screen blocks on granting Accessibility and only proceeds once
// permission_status reports granted. See design-redrafter.md S17 and
// wireframes/onboarding.html.
test.describe('S17: First-run permission gate', () => {
  test('S17: blocks Continue until Accessibility is granted, then proceeds', async ({ page }) => {
    await page.goto('/onboarding');

    const status = page.getByTestId('perm-status');
    await expect(status).toHaveAttribute('data-granted', 'false');
    await expect(status).toContainText('Not granted');

    const continueBtn = page.getByTestId('perm-continue');
    await expect(continueBtn).toBeDisabled();

    // Opening System Settings invokes permission_open_settings on the
    // backend; the mock then simulates the user granting Accessibility.
    await page.getByTestId('perm-open-settings').click();

    await expect(status).toHaveAttribute('data-granted', 'true');
    await expect(status).toContainText('Granted');
    await expect(continueBtn).toBeEnabled();

    await continueBtn.click();
    await expect(page.getByTestId('onboarding-continued')).toBeVisible();
  });
});
