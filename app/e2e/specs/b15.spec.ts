import { test, expect } from '../fixtures/setup';

/**
 * S10: Failure preserves text + fallback — given the active model is
 * unreachable and a fallback chain is configured, when a refine is
 * attempted, then the failure is surfaced (not silently dropped), the
 * original text is never lost (no `inject_text` occurs), and the panel
 * indicates it will try the configured fallback chain. Retrying then
 * succeeds via the same `refine` call. (design-redrafter.md S10,
 * wireframes/capture.html "Error / retry (S10)", wireframes/behavior.html,
 * controls/capture.json `capture-error`/`capture-retry`)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Failure handling and fallback chain (S10)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineFailure: {
        message: "Couldn't connect to http://localhost:11434. Your text is untouched.",
        fallbackModels: ['claude-sonnet-4-6', 'gpt-5.1'],
      },
      refineFailureRepeats: false,
      refineOutcome: {
        original: 'original selection',
        refined: 'refined selection via fallback',
        model: 'claude-sonnet-4-6',
      },
    },
  });

  test('S10: a failed refine surfaces the error with the fallback chain, preserves the original, and a retry succeeds', async ({
    page,
  }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();
    const refineBtn = page.getByTestId('capture-refine');
    await expect(refineBtn).toBeVisible();

    await refineBtn.click();

    // The failure is surfaced -- not a silent drop -- via the capture-error
    // state, with the backend's message.
    const errorState = page.getByTestId('capture-error');
    await expect(errorState).toBeVisible();
    await expect(errorState).toContainText("Couldn't connect to http://localhost:11434. Your text is untouched.");

    // The panel indicates it will try the configured fallback chain.
    const fallbackIndicator = page.getByTestId('capture-error-fallback');
    await expect(fallbackIndicator).toBeVisible();
    await expect(fallbackIndicator).toContainText('claude-sonnet-4-6');
    await expect(fallbackIndicator).toContainText('gpt-5.1');

    const retryBtn = page.getByTestId('capture-retry');
    await expect(retryBtn).toBeVisible();

    // The original text is never lost: the failed call never injected
    // anything, and nothing was restored/pasted at this point either.
    let calls = await mockCalls(page);
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);
    expect(calls.some((c) => c.cmd === 'inject_text')).toBe(false);

    // Retrying (trying the next model in the fallback chain) succeeds.
    await retryBtn.click();

    const doneState = page.getByTestId('capture-done');
    await expect(doneState).toBeVisible();
    await expect(doneState).toContainText('refined selection via fallback');

    // Two refine calls were made overall: the initial failure and the
    // successful retry -- the fallback chain was actually exercised, not
    // just displayed.
    calls = await mockCalls(page);
    const refineCalls = calls.filter((c) => c.cmd === 'refine');
    expect(refineCalls.length).toBe(2);
  });
});
