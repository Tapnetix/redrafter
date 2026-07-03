import { test, expect } from '../fixtures/setup';

/**
 * S35: Progress-feedback options — given the Behavior screen's "Progress
 * feedback" section, when the user toggles the menu-bar spinner, cursor HUD,
 * or completion-sound checkboxes, then each toggle persists via
 * `settings_set` with the right key/value (the exact settings keys
 * `feedback.rs`'s `on_refine_start`/`on_refine_done` read to decide which
 * in-flight cues fire on the next refine), and the chosen options are
 * reflected back in the controls. (design-redrafter.md, wireframes/behavior.html
 * "Progress feedback", controls/behavior.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Behavior: progress feedback options (S35), defaults', () => {
  test('S35: defaults the spinner and sound cues on, the cursor HUD off', async ({ page }) => {
    await page.goto('/behavior');

    const spinner = page.getByTestId('feedback-spinner');
    const hud = page.getByTestId('feedback-hud');
    const sound = page.getByTestId('feedback-sound');

    await expect(spinner).toBeVisible();
    await expect(spinner).toBeChecked();
    await expect(hud).not.toBeChecked();
    await expect(sound).toBeChecked();
  });
});

test.describe('Behavior: progress feedback options (S35), toggling', () => {
  test('S35: turning off the menu-bar spinner persists settings_set with feedback.spinner=false', async ({
    page,
  }) => {
    await page.goto('/behavior');

    const spinner = page.getByTestId('feedback-spinner');
    await expect(spinner).toBeChecked();
    await spinner.click();
    await expect(spinner).not.toBeChecked();

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'feedback.spinner' && c.args.value === 'false',
        ),
      )
      .toBe(true);
  });

  test('S35: turning on the cursor HUD persists settings_set with feedback.hud=true', async ({ page }) => {
    await page.goto('/behavior');

    const hud = page.getByTestId('feedback-hud');
    await expect(hud).not.toBeChecked();
    await hud.click();
    await expect(hud).toBeChecked();

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'feedback.hud' && c.args.value === 'true',
        ),
      )
      .toBe(true);
  });

  test('S35: turning off the completion sound persists settings_set with feedback.sound=false', async ({
    page,
  }) => {
    await page.goto('/behavior');

    const sound = page.getByTestId('feedback-sound');
    await expect(sound).toBeChecked();
    await sound.click();
    await expect(sound).not.toBeChecked();

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'feedback.sound' && c.args.value === 'false',
        ),
      )
      .toBe(true);
  });
});

test.describe('Behavior: progress feedback options (S35), previously saved', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      settings: {
        'feedback.spinner': 'false',
        'feedback.hud': 'true',
        'feedback.sound': 'false',
      },
    },
  });

  test('S35: restores previously persisted feedback toggles from settings_get', async ({ page }) => {
    await page.goto('/behavior');

    await expect(page.getByTestId('feedback-spinner')).not.toBeChecked();
    await expect(page.getByTestId('feedback-hud')).toBeChecked();
    await expect(page.getByTestId('feedback-sound')).not.toBeChecked();
  });
});
