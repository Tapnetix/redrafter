import { test, expect } from '../fixtures/setup';

// S5: Default refine direction. Given a selection with no inline commands,
// a refine is governed by the editable default direction on the Behavior
// screen — a settings value read via settings_get on load and persisted
// via settings_set on edit. (design-redrafter.md S5, wireframes/behavior.html)

test.describe('Behavior: default refine direction (S5), no saved setting', () => {
  test('S5: prefills the built-in default direction when none is saved yet', async ({ page }) => {
    await page.goto('/behavior');

    const textarea = page.getByTestId('default-direction');
    await expect(textarea).toBeVisible();
    await expect(textarea).toHaveValue(
      'Fix grammar, spelling and clarity — keep my voice, tone and length.',
    );
  });
});

test.describe('Behavior: default refine direction (S5), previously saved setting', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      settings: { 'refine.default_direction': 'Make it punchier, keep the facts straight.' },
    },
  });

  test('S5: prefills a previously saved default direction from settings', async ({ page }) => {
    await page.goto('/behavior');

    await expect(page.getByTestId('default-direction')).toHaveValue(
      'Make it punchier, keep the facts straight.',
    );
  });
});

test.describe('Behavior: default refine direction (S5), editing', () => {
  test('S5: editing the default direction persists it via settings_set, preserving voice/length control', async ({
    page,
  }) => {
    await page.goto('/behavior');

    const textarea = page.getByTestId('default-direction');
    const edited = 'Polish grammar only — never shorten, never add jokes.';
    await textarea.fill(edited);
    // Blur so the save handler fires, mirroring a real user tabbing away.
    await page.getByTestId('default-language-note').click();

    await expect(textarea).toHaveValue(edited);

    const calls = await page.evaluate(() => window.__TAURI_MOCK_CALLS__);
    const saveCall = calls.find(
      (call: { cmd: string; args?: { key?: string; value?: string } }) =>
        call.cmd === 'settings_set' && call.args?.key === 'refine.default_direction',
    );
    expect(saveCall).toBeTruthy();
    expect(saveCall?.args?.value).toBe(edited);
  });
});
