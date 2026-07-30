import { test, expect } from '../fixtures/setup';

/**
 * S25: Favorite a model — given enabled models on the Models screen, when the
 * user stars one via its `model-favorite-*` toggle, then `model_toggle_favorite`
 * fires and the model surfaces as favorited (the star fills in and
 * `models_list`'s `favorite` flag flips true — the same field the menu-bar
 * tray's quick-switch favorites section reads from). Un-starring it flips the
 * flag back off. (wireframes/models.html, controls/models.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Favorite a model (S25)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [
        {
          id: '1',
          providerKind: 'anthropic',
          baseUrl: 'https://api.anthropic.com',
          enabledModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
          availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
          keyRef: '1',
        },
      ],
      activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
      // claude-sonnet-4-6 starts un-favorited; the spec stars it, then
      // un-stars it, to exercise both directions of the toggle.
      favoriteModels: [{ connectionId: '1', modelId: 'claude-opus-4-6' }],
    },
  });

  test('S25: starring a model marks it favorite via model_toggle_favorite and un-starring clears it', async ({
    page,
  }) => {
    await page.goto('/');
    await page.getByTestId('nav-models').click();
    await expect(page.getByTestId('models-table')).toBeVisible();

    const opusStar = page.getByTestId('model-favorite-claude-opus-4-6');
    const sonnetStar = page.getByTestId('model-favorite-claude-sonnet-4-6');

    // Seeded state: opus starts favorited, sonnet does not.
    await expect(opusStar).toHaveAttribute('aria-pressed', 'true');
    await expect(opusStar).toHaveText('★');
    await expect(sonnetStar).toHaveAttribute('aria-pressed', 'false');
    await expect(sonnetStar).toHaveText('☆');

    // Star claude-sonnet-4-6.
    await sonnetStar.click();

    await expect(sonnetStar).toHaveAttribute('aria-pressed', 'true');
    await expect(sonnetStar).toHaveText('★');

    const toggleCalls = (await mockCalls(page)).filter((c) => c.cmd === 'model_toggle_favorite');
    expect(toggleCalls).toHaveLength(1);
    expect(toggleCalls[0].args.connectionId).toBe('1');
    expect(toggleCalls[0].args.modelId).toBe('claude-sonnet-4-6');

    // The tray's quick-switch favorites section reads the same
    // `models_list().favorite` field this toggle just flipped — reload the
    // list (as the tray would) and confirm the favorited model is present.
    const afterFavorite = await page.evaluate(() =>
      (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke(
        'models_list',
      ),
    );
    const models = (afterFavorite as { models: { modelId: string; favorite: boolean }[] }).models;
    expect(models.find((m) => m.modelId === 'claude-sonnet-4-6')?.favorite).toBe(true);
    expect(models.find((m) => m.modelId === 'claude-opus-4-6')?.favorite).toBe(true);

    // Un-star claude-opus-4-6 to exercise the other direction of the toggle.
    await opusStar.click();

    await expect(opusStar).toHaveAttribute('aria-pressed', 'false');
    await expect(opusStar).toHaveText('☆');

    const allToggleCalls = (await mockCalls(page)).filter((c) => c.cmd === 'model_toggle_favorite');
    expect(allToggleCalls).toHaveLength(2);
    expect(allToggleCalls[1].args.connectionId).toBe('1');
    expect(allToggleCalls[1].args.modelId).toBe('claude-opus-4-6');

    const afterUnfavorite = await page.evaluate(() =>
      (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke(
        'models_list',
      ),
    );
    const modelsAfter = (afterUnfavorite as { models: { modelId: string; favorite: boolean }[] }).models;
    expect(modelsAfter.find((m) => m.modelId === 'claude-opus-4-6')?.favorite).toBe(false);
    expect(modelsAfter.find((m) => m.modelId === 'claude-sonnet-4-6')?.favorite).toBe(true);
  });
});
