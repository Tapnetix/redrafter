import { test, expect } from '../fixtures/setup';

/**
 * The in-flight / failure chip (`app/src-tauri/src/hud.rs`).
 *
 * The failure half is the point: a refine triggered by the global hotkey hands
 * its Result to a JoinHandle the shortcut handler discards, so an error was
 * never read, logged or shown anywhere. The spinner appearing and vanishing
 * was the entire user-visible account of a failed refine.
 */

test.describe('In-flight chip', () => {
  test.use({ testData: { appName: 'redrafter', permissionGranted: true, hudState: { kind: 'refining', text: '' } } });

  test('shows the spinner while a refine is running', async ({ page }) => {
    await page.goto('/hud');

    const chip = page.getByTestId('hud-chip');
    await expect(chip).toHaveAttribute('data-kind', 'refining');
    await expect(chip).toContainText('Refining');
    await expect(page.getByTestId('hud-error')).toHaveCount(0);
  });
});

test.describe('Failure chip', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      hudState: { kind: 'error', text: '401 Unauthorized: credit balance is too low' },
    },
  });

  test('says what went wrong instead of vanishing silently', async ({ page }) => {
    await page.goto('/hud');

    const chip = page.getByTestId('hud-chip');
    await expect(chip).toHaveAttribute('data-kind', 'error');
    await expect(page.getByTestId('hud-error')).toContainText('credit balance is too low');
  });

  test('keeps the full message reachable when it is too long to fit', async ({ page }) => {
    await page.goto('/hud');

    // The chip is small; the untruncated text stays available as a tooltip.
    await expect(page.getByTestId('hud-error')).toHaveAttribute(
      'title',
      '401 Unauthorized: credit balance is too low',
    );
  });
});
