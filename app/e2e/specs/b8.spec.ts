import { test, expect } from '../fixtures/setup';

/**
 * S8: Model curation and active selection — given a connection with a
 * discovered-but-not-yet-enabled model, when the user enables it (from
 * Connections) and then picks it as the active model on the Models screen,
 * then it becomes the single global active model — the one `refine` will
 * use — and the previously active model is no longer marked active.
 * (design-redrafter.md, wireframes/models.html, controls/models.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Model curation and active selection (S8)', () => {
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
          availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
          keyRef: '1',
        },
      ],
      activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
    },
  });

  test('S8: enabling a discovered model and setting it active makes it the model refine will use', async ({
    page,
  }) => {
    // Use the real app shell (not a standalone route) so navigating from
    // Connections to Models is in-app and shares the same mock backend
    // state, rather than a fresh page load re-seeding from TEST_DATA.
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    // Enable the already-discovered claude-sonnet-4-6 from Connections.
    await page.getByTestId('rail-connections').click();
    await page.getByTestId('connection-edit-anthropic').click();
    const sonnetCheck = page.getByTestId('conn-model-check-claude-sonnet-4-6');
    await expect(sonnetCheck).not.toBeChecked();
    await sonnetCheck.click();
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_edit'))
      .toBe(true);
    await page.getByTestId('connection-cancel').click();

    // Follow the "Models" link to curate/activate it there.
    await page.getByTestId('connections-models-link').click();
    await expect(page.getByTestId('models-table')).toBeVisible();

    // Both models are now enabled; claude-opus-4-6 is the seeded active one.
    const opusRadio = page.getByTestId('model-active-radio-claude-opus-4-6');
    const sonnetRadio = page.getByTestId('model-active-radio-claude-sonnet-4-6');
    await expect(opusRadio).toBeVisible();
    await expect(sonnetRadio).toBeVisible();
    await expect(opusRadio).toHaveAttribute('aria-checked', 'true');
    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'false');

    // Set claude-sonnet-4-6 as the active model.
    await sonnetRadio.click();

    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'true');
    await expect(opusRadio).toHaveAttribute('aria-checked', 'false');

    const activeCall = (await mockCalls(page)).find((c) => c.cmd === 'model_set_active');
    expect(activeCall).toBeTruthy();
    expect(activeCall!.args.connectionId).toBe('1');
    expect(activeCall!.args.modelId).toBe('claude-sonnet-4-6');

    // No stale/unavailable banners: a valid active model is set.
    await expect(page.getByTestId('models-no-active-banner')).toHaveCount(0);
    await expect(page.getByTestId('model-active-unavailable')).toHaveCount(0);
  });
});
