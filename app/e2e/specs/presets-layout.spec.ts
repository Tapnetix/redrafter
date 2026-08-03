import { test, expect } from '../fixtures/setup';

/**
 * The Presets editor has to use the window it is given.
 *
 * The screen is a master/detail layout but reused `.settings`, whose
 * `max-width: 760px` was picked for the single-column forms that share the
 * class. The list took a fixed 280px and the editor was left with roughly
 * 420px — narrow enough to clip the "Model override" select mid-word. Worse,
 * the cap is absolute: widening the window grew the empty gutters either side
 * and left the box exactly the same size.
 *
 * So the assertions are about observable geometry, not markup: the editor is
 * wide enough to hold its own controls, and it grows when the window does.
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
    activeModel: { connectionId: '1', modelId: 'claude-sonnet-5' },
  },
});

/**
 * Opens Presets at `width`. Navigation happens wide — below 861px the sidebar
 * gives way to the icon rail, and which nav is clickable is not what these
 * tests are about (that is `layout.spec.ts`) — then the window is resized to
 * the width under test.
 */
async function openPresets(page: import('@playwright/test').Page, width: number, height = 800) {
  await page.setViewportSize({ width: Math.max(width, 1000), height });
  await page.goto('/');
  await expect(page.getByTestId('app-shell')).toBeVisible();
  await page.getByTestId('nav-presets').click();
  await expect(page.getByTestId('presets-screen')).toBeVisible();
  if (width < 1000) {
    await page.setViewportSize({ width, height });
    await expect(page.getByTestId('preset-editor')).toBeVisible();
  }
}

async function widthOf(page: import('@playwright/test').Page, testId: string): Promise<number> {
  const box = await page.getByTestId(testId).boundingBox();
  expect(box).not.toBeNull();
  return box!.width;
}

test('the editor grows when the window does', async ({ page }) => {
  await openPresets(page, 900);
  const narrow = await widthOf(page, 'preset-editor');

  await page.setViewportSize({ width: 1400, height: 800 });
  // Give the layout a frame to settle before measuring again.
  await expect(page.getByTestId('preset-editor')).toBeVisible();
  const wide = await widthOf(page, 'preset-editor');

  // 500px more window has to reach the editor. The old fixed 760px shell gave
  // it back exactly 0.
  expect(wide).toBeGreaterThan(narrow + 400);
});

test('the editor is wide enough for the controls it contains', async ({ page }) => {
  await openPresets(page, 1280);

  // Roomy enough for two fields side by side plus the panel's own padding —
  // the old layout managed ~420px here.
  expect(await widthOf(page, 'preset-editor')).toBeGreaterThan(600);

  // The select must be able to show its longest value. "Inherit (active
  // model)" rendered as "Inherit (active mo" before.
  const select = page.getByTestId('preset-model');
  const fits = await select.evaluate((el) => {
    const probe = document.createElement('span');
    const style = getComputedStyle(el);
    probe.style.cssText = `position:absolute;visibility:hidden;white-space:pre;font:${style.font}`;
    probe.textContent = (el as HTMLSelectElement).selectedOptions[0].text;
    document.body.appendChild(probe);
    const textWidth = probe.getBoundingClientRect().width;
    probe.remove();
    // Padding, border and the disclosure arrow all eat into the box.
    const chrome =
      parseFloat(style.paddingLeft) + parseFloat(style.paddingRight) + parseFloat(style.borderLeftWidth) +
      parseFloat(style.borderRightWidth) + 20;
    return el.getBoundingClientRect().width - chrome >= textWidth;
  });
  expect(fits).toBe(true);
});

test('the list and editor stack rather than crush each other when narrow', async ({ page }) => {
  await openPresets(page, 620);

  const list = await page.getByTestId('preset-list').boundingBox();
  const editor = await page.getByTestId('preset-editor').boundingBox();
  expect(list).not.toBeNull();
  expect(editor).not.toBeNull();
  // Stacked: the editor starts below the list, not beside it.
  expect(editor!.y).toBeGreaterThan(list!.y + list!.height - 1);
});

test('nothing in the editor overflows the window', async ({ page }) => {
  for (const width of [620, 900, 1280, 1600]) {
    await openPresets(page, width);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `horizontal overflow at ${width}px`).toBeLessThanOrEqual(0);
  }
});
