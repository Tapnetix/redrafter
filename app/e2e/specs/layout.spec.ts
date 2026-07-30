import { test, expect } from '../fixtures/setup';

/**
 * Exactly one navigation, at any width.
 *
 * The shell used to render both the icon rail and the sidebar above the 860px
 * breakpoint — two complete navigations listing the same six sections, one as
 * icons and one as icons with labels. It only became obvious once the window
 * was widened, which is also what made the default window size worth raising.
 */

test.use({
  testData: {
    appName: 'redrafter',
    permissionGranted: true,
    connections: [
      {
        id: '1',
        providerKind: 'anthropic',
        baseUrl: 'https://api.anthropic.com',
        enabledModels: ['claude-sonnet-5'],
        availableModels: ['claude-sonnet-5'],
        keyRef: '1',
      },
    ],
  },
});

test('wide: the sidebar navigates and the rail steps aside', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.goto('/');

  await expect(page.getByTestId('sidebar')).toBeVisible();
  await expect(page.getByTestId('icon-rail')).toBeHidden();
  // Every section reachable exactly once.
  await expect(page.getByTestId('nav-models')).toBeVisible();
  await expect(page.getByTestId('rail-models')).toBeHidden();
  // The toggle lives in the topbar now, so it survives the rail being hidden.
  await expect(page.getByTestId('theme-toggle')).toBeVisible();
});

test('narrow: the rail navigates and the sidebar steps aside', async ({ page }) => {
  await page.setViewportSize({ width: 520, height: 700 });
  await page.goto('/');

  await expect(page.getByTestId('icon-rail')).toBeVisible();
  await expect(page.getByTestId('sidebar')).toBeHidden();
  await expect(page.getByTestId('rail-models')).toBeVisible();
  await expect(page.getByTestId('theme-toggle')).toBeVisible();
});

test('navigation works from whichever nav is showing', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.goto('/');
  await page.getByTestId('nav-behavior').click();
  await expect(page.getByTestId('behavior-default-direction')).toBeVisible();

  await page.setViewportSize({ width: 520, height: 700 });
  await page.getByTestId('rail-history').click();
  await expect(page.getByTestId('history-screen')).toBeVisible();
});
