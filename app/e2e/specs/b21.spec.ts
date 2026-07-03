import { test, expect } from '../fixtures/setup';

/**
 * S26: Disable model and active-unavailable — given a connection whose
 * currently-active model is disabled from the Models screen, then the app
 * doesn't silently keep refining with the now-disabled model: it drops into
 * an explicit "active model unavailable" state (a banner naming the stale
 * model and prompting the user to pick a new active model), and picking a
 * new active model clears that banner. (design-redrafter.md,
 * wireframes/models.html, controls/models.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Disable model and active-unavailable (S26)', () => {
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
    },
  });

  test('S26: disabling the active model surfaces an active-unavailable banner prompting re-selection', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('rail-models').click();
    await expect(page.getByTestId('models-table')).toBeVisible();

    const opusRadio = page.getByTestId('model-active-radio-claude-opus-4-6');
    await expect(opusRadio).toHaveAttribute('aria-checked', 'true');

    // No banner yet: the active model is valid.
    await expect(page.getByTestId('model-active-unavailable')).toHaveCount(0);

    // Disable the currently-active model.
    await page.getByTestId('model-disable-claude-opus-4-6').click();

    const disableCall = (await mockCalls(page)).find((c) => c.cmd === 'model_disable');
    expect(disableCall).toBeTruthy();
    expect(disableCall!.args.connectionId).toBe('1');
    expect(disableCall!.args.modelId).toBe('claude-opus-4-6');

    // The disabled model drops out of the table entirely...
    await expect(page.getByTestId('model-active-radio-claude-opus-4-6')).toHaveCount(0);
    // ...and rather than silently refining with a disabled model, the
    // active-unavailable banner appears, naming the stale model and
    // prompting the user to pick a new active model.
    const banner = page.getByTestId('model-active-unavailable');
    await expect(banner).toBeVisible();
    await expect(banner).toContainText('claude-opus-4-6');
    await expect(page.getByTestId('models-no-active-banner')).toHaveCount(0);

    // Picking a new active model clears the banner.
    const sonnetRadio = page.getByTestId('model-active-radio-claude-sonnet-4-6');
    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'false');
    await sonnetRadio.click();

    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'true');
    await expect(page.getByTestId('model-active-unavailable')).toHaveCount(0);
  });
});
