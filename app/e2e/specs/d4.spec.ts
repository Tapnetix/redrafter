import { test, expect } from '../fixtures/setup';

/**
 * S38: Cross-platform refine loop — given the built app on macOS, Linux
 * (X11 and Wayland), and Windows, when the user runs the hotkey refine loop
 * and opens the settings flows, then both work on each platform.
 * (design-redrafter.md S38, wireframes/capture.html, wireframes/index.html)
 *
 * This spec is the mock-level breadth check, NOT the S38 acceptance gate —
 * see design-redrafter.md's "Acceptance Tooling": Playwright + IPC mocks
 * only ever prove the frontend renders/wires correctly against a given
 * backend response shape, never that the real per-OS backend (AX on macOS,
 * X11/Wayland clipboard+XTEST/ydotool on Linux, UIA/SendInput on Windows —
 * D1/D2/D3) actually works on that OS. What IS checkable in this headless
 * mock harness, and what this spec asserts, is the cross-platform
 * *contract*: D3 made `permission_status`/`permission_open_settings`/
 * `hotkey_set`/tray commands behave uniformly across cfg branches (a no-op
 * granted permission on Linux/Windows vs the real interactive AX grant on
 * macOS), so the exact same frontend code path (Onboarding.tsx, Capture.tsx,
 * General/Connections/Models screens) must route through to the same ready
 * state regardless of which platform's permission semantics produced
 * `granted`. The two `S38: onboarding` tests below drive both shapes of
 * that contract (instant-granted vs interactive-granted) through the
 * identical UI and assert they converge; the loop/settings test then
 * exercises the core capture -> refine -> inject flow (S1) and a settings
 * flow (connection add + set active model) once permission is granted,
 * exactly as it would run identically on every platform.
 *
 * The full S38 acceptance -- the packaged binary actually driven by
 * tauri-driver + WebdriverIO on Linux (X11 and Wayland) and Windows, and
 * the macOS launch-smoke against a real focused text field -- lives in
 * `e2e-real/` (see `e2e-real/README.md`) and is run by hand on each target
 * OS; it cannot run on this headless Linux CI host (no packaged binary, no
 * live display, no macOS/Windows).
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('S38: cross-platform permission gate', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      // Mirrors Linux/Windows (D3): permission_status is a no-op-granted
      // stub, so there is nothing to interactively grant -- Continue must
      // already be enabled on first render.
      permissionGranted: true,
    },
  });

  test('S38: an instant-granted platform (Linux/Windows no-op permission) reaches Continue immediately', async ({
    page,
  }) => {
    await page.goto('/onboarding');

    const status = page.getByTestId('perm-status');
    await expect(status).toHaveAttribute('data-granted', 'true');
    await expect(status).toContainText('Granted');

    const continueBtn = page.getByTestId('perm-continue');
    await expect(continueBtn).toBeEnabled();

    await continueBtn.click();
    await expect(page.getByTestId('onboarding-continued')).toBeVisible();
  });
});

test.describe('S38: cross-platform permission gate (interactive grant)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      // Mirrors macOS (D3): permission_status starts ungranted until the
      // user grants Accessibility in System Settings.
      permissionGranted: false,
    },
  });

  test('S38: an interactive-grant platform (macOS Accessibility) reaches the same Continue state after granting', async ({
    page,
  }) => {
    await page.goto('/onboarding');

    const status = page.getByTestId('perm-status');
    await expect(status).toHaveAttribute('data-granted', 'false');

    const continueBtn = page.getByTestId('perm-continue');
    await expect(continueBtn).toBeDisabled();

    await page.getByTestId('perm-open-settings').click();

    await expect(status).toHaveAttribute('data-granted', 'true');
    await expect(continueBtn).toBeEnabled();

    await continueBtn.click();
    // Same terminal state as the instant-granted platform above: the
    // permission model differs per-OS, but the UI contract it feeds into
    // does not.
    await expect(page.getByTestId('onboarding-continued')).toBeVisible();
  });
});

test.describe('S38: cross-platform refine loop and settings flow', () => {
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
      refineOutcome: {
        original: 'the linux build works good on wayland i think',
        refined: 'The Linux build works well on Wayland.',
        model: 'claude-opus-4-6',
      },
    },
  });

  test('S38: hotkey capture -> refine -> inject and a settings flow (set active model) both run through the platform-uniform IPC contract', async ({
    page,
  }) => {
    // Core S1 loop: capture -> refine -> blind-inject, unchanged by which
    // platform's text-inject backend (macOS AX / Linux X11-Wayland /
    // Windows UIA-SendInput) the real `refine` command would drive.
    await page.goto('/capture');
    await expect(page.getByTestId('capture-panel')).toBeVisible();

    await page.getByTestId('capture-refine').click();
    await expect(page.getByText('The Linux build works well on Wayland.')).toBeVisible();

    const calls = await mockCalls(page);
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);

    // Settings flow: navigating Connections -> Models and setting the
    // active model exercises the same settings surface every platform's
    // build ships (D3 kept the tray/hotkey/permission screens building and
    // running everywhere).
    await page.goto('/');
    await expect(page.getByTestId('app-shell')).toBeVisible();

    await page.getByTestId('nav-connections').click();
    await page.getByTestId('connections-models-link').click();
    await expect(page.getByTestId('models-table')).toBeVisible();

    const sonnetRadio = page.getByTestId('model-active-radio-claude-sonnet-4-6');
    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'false');
    await sonnetRadio.click();
    await expect(sonnetRadio).toHaveAttribute('aria-checked', 'true');

    const activeCall = (await mockCalls(page)).find((c) => c.cmd === 'model_set_active');
    expect(activeCall).toBeTruthy();
    expect(activeCall!.args.modelId).toBe('claude-sonnet-4-6');
  });
});
