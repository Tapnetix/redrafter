import { test, expect } from '../fixtures/setup';

/**
 * S16: History retention policy — given the Behavior screen's "History
 * retention" section, when the user sets the max-entries-to-keep and/or
 * auto-purge-after-days controls, then each choice persists via
 * `settings_set` with the exact keys C5 wired (`history.retention_count` /
 * `history.retention_days`), and a reload restores the previously persisted
 * choice via `settings_get`. Actual pruning of existing history entries
 * against this policy is a backend concern (C17's job to wire into
 * run_refine/append); this spec covers the user-facing configuration
 * persistence at the settings layer. (design-redrafter.md,
 * wireframes/behavior.html "History retention", controls/behavior.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Behavior: history retention (S16), defaults', () => {
  test('S16: defaults to keeping 50 entries and purging after 7 days', async ({ page }) => {
    await page.goto('/behavior');

    const count = page.getByTestId('retention-count');
    const days = page.getByTestId('retention-days');

    await expect(count).toBeVisible();
    await expect(count).toHaveValue('50');
    await expect(days).toBeVisible();
    await expect(days).toHaveValue('7');
  });
});

test.describe('Behavior: history retention (S16), setting the policy', () => {
  test('S16: changing the max entries to keep persists settings_set with history.retention_count', async ({
    page,
  }) => {
    await page.goto('/behavior');

    const count = page.getByTestId('retention-count');
    await count.selectOption('200');
    await expect(count).toHaveValue('200');

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'history.retention_count' && c.args.value === '200',
        ),
      )
      .toBe(true);
  });

  test('S16: changing the auto-purge age persists settings_set with history.retention_days', async ({ page }) => {
    await page.goto('/behavior');

    const days = page.getByTestId('retention-days');
    await days.selectOption('30');
    await expect(days).toHaveValue('30');

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'history.retention_days' && c.args.value === '30',
        ),
      )
      .toBe(true);
  });

  test('S16: setting retention to session-only (0 days) persists settings_set with history.retention_days=0', async ({
    page,
  }) => {
    await page.goto('/behavior');

    const days = page.getByTestId('retention-days');
    await days.selectOption('0');
    await expect(days).toHaveValue('0');

    await expect
      .poll(async () =>
        (await mockCalls(page)).some(
          (c) => c.cmd === 'settings_set' && c.args.key === 'history.retention_days' && c.args.value === '0',
        ),
      )
      .toBe(true);
  });
});

test.describe('Behavior: history retention (S16), previously saved', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      settings: {
        'history.retention_count': 'Unlimited',
        'history.retention_days': '90',
      },
    },
  });

  test('S16: restores the previously persisted retention policy from settings_get on reload', async ({ page }) => {
    await page.goto('/behavior');

    await expect(page.getByTestId('retention-count')).toHaveValue('Unlimited');
    await expect(page.getByTestId('retention-days')).toHaveValue('90');
  });
});
