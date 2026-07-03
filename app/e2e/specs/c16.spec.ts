import { test, expect } from '../fixtures/setup';

/**
 * S39: Tray reflects refine error — given a refine has just failed, when the
 * user looks at the menu-bar tray, then the tray's status line shows the
 * error state (`tray-state-error`) with the failure reflected in the text,
 * and "Refine selection" (`tray-refine`) stays available so the user can
 * retry. (design-redrafter.md S39, wireframes/tray.html "error status after
 * failed refine", controls/tray.json `tray-refine` -> `tray_refine`)
 *
 * Drives the tray into its error state via the tray's own Refine entry
 * point (`tray_refine`) rejecting with the `no_active_model` sentinel — the
 * existing seam Tray.tsx's `handleRefine` already funnels into the generic
 * `tray-state-error` status (B17), rather than adding a new mock fixture.
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Tray error status (S39)', () => {
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
          keyRef: '1',
        },
      ],
      activeModel: { connectionId: '1', modelId: 'claude-opus-4-6' },
      refineError: 'no_active_model',
    },
  });

  test('S39: a failed tray refine shows the error status and keeps retry available', async ({ page }) => {
    await page.goto('/tray');

    // Starts Ready -- no error yet.
    await expect(page.getByTestId('tray-state-idle')).toBeVisible();
    await expect(page.getByTestId('tray-state-error')).toHaveCount(0);

    // Trigger the refine from the tray's own entry point; the mocked
    // `tray_refine` rejects (mirrors an unreachable model / backend
    // failure), so nothing gets injected.
    await page.getByTestId('tray-refine').click();

    await expect.poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_refine').length).toBe(1);

    // The tray now reflects the ERROR status, not idle/refining/paused --
    // the user can tell something went wrong straight from the menu bar.
    const errorState = page.getByTestId('tray-state-error');
    await expect(errorState).toBeVisible();
    await expect(errorState).toContainText('Last refine failed');
    await expect(page.getByTestId('tray-state-idle')).toHaveCount(0);

    // A retry stays available: Refine selection is still enabled (not
    // disabled the way it is while paused) and invoking it again calls
    // `tray_refine` a second time.
    const refineBtn = page.getByTestId('tray-refine');
    await expect(refineBtn).toBeEnabled();

    await refineBtn.click();
    await expect.poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_refine').length).toBe(2);
    await expect(page.getByTestId('tray-state-error')).toBeVisible();
  });
});
