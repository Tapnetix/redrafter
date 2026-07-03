import { test, expect } from '../fixtures/setup';

/**
 * S22: Tray status and pause — given the menu-bar tray, when the user pauses
 * global capturing (`tray-pause`), then `tray_pause` fires, the status line
 * switches to "Paused", and the tray's own "Refine selection" action no
 * longer calls `tray_refine`; resuming (`tray-resume`) calls `tray_resume`,
 * restores the "Ready" status, and lets Refine selection call `tray_refine`
 * again. (wireframes/tray.html, controls/tray.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Tray status and pause (S22)', () => {
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
    },
  });

  test('S22: pausing suspends refine and shows Paused; resuming restores it', async ({ page }) => {
    await page.goto('/tray');

    // Starts ready, not paused: the Pause control is visible, Resume is not.
    await expect(page.getByTestId('tray-state-idle')).toBeVisible();
    await expect(page.getByTestId('tray-pause')).toBeVisible();
    await expect(page.getByTestId('tray-resume')).toHaveCount(0);
    await expect(page.getByTestId('tray-state-paused')).toHaveCount(0);

    // Refine selection works normally before pausing.
    await page.getByTestId('tray-refine').click();
    await expect.poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_refine').length).toBe(1);

    // Pause: fires tray_pause, flips the status to Paused, and swaps the
    // Pause control for Resume.
    await page.getByTestId('tray-pause').click();

    await expect.poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'tray_pause')).toBe(true);
    await expect(page.getByTestId('tray-state-paused')).toBeVisible();
    await expect(page.getByTestId('tray-state-paused')).toContainText('Paused');
    await expect(page.getByTestId('tray-resume')).toBeVisible();
    await expect(page.getByTestId('tray-pause')).toHaveCount(0);

    // While paused, Refine selection is disabled and no longer invokes
    // tray_refine.
    await expect(page.getByTestId('tray-refine')).toBeDisabled();
    expect((await mockCalls(page)).filter((c) => c.cmd === 'tray_refine')).toHaveLength(1);

    // Resume: fires tray_resume, restores Ready, and Refine works again.
    await page.getByTestId('tray-resume').click();

    await expect.poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'tray_resume')).toBe(true);
    await expect(page.getByTestId('tray-state-idle')).toBeVisible();
    await expect(page.getByTestId('tray-state-paused')).toHaveCount(0);
    await expect(page.getByTestId('tray-pause')).toBeVisible();

    await page.getByTestId('tray-refine').click();
    await expect.poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'tray_refine').length).toBe(2);
  });
});
