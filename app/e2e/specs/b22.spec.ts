import { test, expect } from '../fixtures/setup';

/**
 * S27: Ollama model pull — given an Ollama connection, when the user types
 * a model name into the Models screen's "Get more Ollama models" field and
 * clicks Pull, then `ollama_pull` fires with that model name, the screen
 * renders a progress/done state, and — on success — the pulled model
 * becomes available to enable/curate (surfacing in the Connections screen's
 * discovered-models checklist for that Ollama connection), without being
 * auto-enabled. (design-redrafter.md, wireframes/models.html,
 * controls/models.json: `ollama-pull-field` / `ollama-pull-start`)
 *
 * Per B8/B1 the pull itself is a single command that resolves once the
 * backend's streamed NDJSON progress reaches its terminal line — this spec
 * asserts that resolved SUCCESS path (a `pulling` UI state also exists for
 * the in-flight window, but is too transient around the mocked resolution
 * to assert reliably; see B8's known truncated-stream edge case, which this
 * spec deliberately does not exercise).
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Ollama model pull (S27)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      permissionGranted: true,
      connections: [
        {
          id: 'ollama-1',
          providerKind: 'ollama',
          baseUrl: 'http://localhost:11434',
          enabledModels: ['llama3.1:8b'],
          availableModels: ['llama3.1:8b'],
        },
      ],
      activeModel: { connectionId: 'ollama-1', modelId: 'llama3.1:8b' },
    },
  });

  test('S27: pulling an Ollama model by name streams to success and the model becomes available', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-models').click();
    await expect(page.getByTestId('models-table')).toBeVisible();

    // Not yet discovered/available anywhere: pulling it is the only way it
    // shows up.
    await expect(page.getByTestId('model-active-radio-phi4')).toHaveCount(0);

    await page.getByTestId('ollama-pull-idle').waitFor();
    await page.getByTestId('ollama-pull-field').fill('phi4');
    await page.getByTestId('ollama-pull-start').click();

    // The exact backend command named in controls/models.json fires with
    // the typed model name.
    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'ollama_pull' && c.args.modelId === 'phi4'))
      .toBe(true);

    // Terminal (success) state renders, replacing the idle placeholder.
    await expect(page.getByTestId('ollama-pull-idle')).toHaveCount(0);
    await expect(page.getByTestId('ollama-pull-error')).toHaveCount(0);
    const done = page.getByTestId('ollama-pull-done');
    await expect(done).toBeVisible();
    await expect(done).toContainText('phi4');
    await expect(done).toContainText('available');

    // The model becomes available to enable/curate — surfacing in the
    // Ollama connection's discovered-models checklist on Connections —
    // without being auto-enabled (B1's `ollama_pull` only refreshes
    // availability; enabling still happens explicitly, same as any other
    // discovered model).
    await page.getByTestId('nav-connections').click();
    await page.getByTestId('connection-edit-ollama').click();
    const phi4Check = page.getByTestId('conn-model-check-phi4');
    await expect(phi4Check).toBeVisible();
    await expect(phi4Check).not.toBeChecked();
    await page.getByTestId('connection-cancel').click();
  });
});
