import { test, expect } from '../fixtures/setup';

/**
 * S32: History search — given a populated History screen, when the user
 * types into the search box, then the list is filtered client-side to only
 * the entries matching that text; clearing the search restores the full
 * list, and a search with no matches shows the no-results state instead of
 * the row list. (design-redrafter.md, wireframes/history.html,
 * controls/history.json)
 */

test.describe('History search (S32)', () => {
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
        },
      ],
      historyEntries: [
        {
          id: '2',
          original: 'thanks, sounds good, will check the numbers tomorrow',
          refined: "Thanks — sounds good. I'll check the numbers tomorrow.",
          model: 'claude-opus-4-6',
          createdAt: Date.now() - 60_000,
        },
        {
          id: '1',
          original: 'we good with the release plan i think, shipping monday probably',
          refined: "We're good with the release plan — on track to ship Monday.",
          model: 'claude-opus-4-6',
          createdAt: Date.now() - 120_000,
        },
      ],
    },
  });

  test('S32: typing a search term filters the list to matching entries', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await expect(page.getByTestId('history-row')).toHaveCount(2);

    await page.getByTestId('history-search').fill('release plan');

    const rows = page.getByTestId('history-row');
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText('release plan');
  });

  test('S32: clearing the search restores the full list', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await page.getByTestId('history-search').fill('release plan');
    await expect(page.getByTestId('history-row')).toHaveCount(1);

    await page.getByTestId('history-search').fill('');

    await expect(page.getByTestId('history-row')).toHaveCount(2);
  });

  test('S32: a search with no matches shows the no-results state', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-history').click();
    await expect(page.getByTestId('history-row')).toHaveCount(2);

    await page.getByTestId('history-search').fill('nothing in history matches this term');

    await expect(page.getByTestId('history-no-results')).toBeVisible();
    await expect(page.getByTestId('history-row')).toHaveCount(0);
  });
});
