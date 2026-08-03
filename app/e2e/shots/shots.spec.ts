import { test } from '../fixtures/setup';

// Screenshot generation for the README. Runs against the *static export*
// (out/) rather than the dev server, so there is no Next.js dev overlay.

const W = 1280;
const H = 860;

test.use({
  testData: {
    appName: 'redrafter',
    permissionGranted: true,
    settings: { theme: 'dark' },
    connections: [
      {
        id: '1',
        providerKind: 'anthropic',
        baseUrl: 'https://api.anthropic.com',
        enabledModels: ['claude-sonnet-5', 'claude-opus-5'],
        availableModels: ['claude-sonnet-5', 'claude-opus-5', 'claude-haiku-4-5'],
        keyRef: '1',
      },
      {
        id: '2',
        providerKind: 'ollama',
        baseUrl: 'http://localhost:11434',
        enabledModels: ['qwen3:8b'],
        availableModels: ['qwen3:8b', 'llama3.2:3b'],
        keyRef: null,
      },
    ],
    activeModel: { connectionId: '1', modelId: 'claude-sonnet-5' },
    refineOutcome: {
      original: 'sorry for the delay, we shipped friday and i meant to tell you sooner',
      refined:
        "Apologies for the delay \u2014 we shipped on Friday, and I meant to let you know sooner.",
      model: 'claude-sonnet-5',
    },
    historyEntries: [
      {
        id: 'h1',
        original: 'we good with the release plan i think, no delays i hope, shipping monday probably',
        refined: "We're good with the release plan — no delays. On track to ship Monday.",
        model: 'claude-sonnet-5',
        createdAt: 1785748320000,
        command: null,
      },
      {
        id: 'h2',
        original: 'sorry for the delay, we shipped friday',
        refined: 'Entschuldigen Sie die Verzögerung — wir haben am Freitag ausgeliefert.',
        model: 'claude-sonnet-5',
        createdAt: 1785674400000,
        command: '/lang de',
      },
      {
        id: 'h3',
        original: 'can you take a look at the ticket when you get a sec',
        refined: 'Could you take a look at the ticket when you have a moment?',
        model: 'qwen3:8b',
        createdAt: 1785654300000,
        command: '/friendly',
      },
    ],
  },
});

/**
 * Opens a screen and fits the window to it, so a short screen isn't padded
 * with dead space and a tall one isn't cut off mid-control. `.content` is the
 * only scrolling region; everything else is fixed shell chrome.
 */
async function open(page: import('@playwright/test').Page, nav: string) {
  await page.setViewportSize({ width: W, height: H });
  await page.goto('/index.html');
  await page.getByTestId('app-shell').waitFor();
  await page.getByTestId(nav).click();
  // Let fonts and any async settings read settle before measuring.
  await page.waitForTimeout(400);

  const needed = await page.evaluate(() => {
    const topbar = document.querySelector('.topbar');
    const chrome = topbar ? topbar.getBoundingClientRect().height : 0;
    // Measure the screen itself, not `.content` — `.content` is `flex: 1` in a
    // 100vh column, so its scrollHeight is at least the viewport and would
    // just report back whatever height we came in with.
    const inner = document.querySelector('.content > *');
    const body = inner ? inner.getBoundingClientRect().height : 0;
    return Math.ceil(chrome + body + 24);
  });
  await page.setViewportSize({ width: W, height: Math.min(Math.max(needed, 520), 1500) });
  await page.waitForTimeout(200);
}

test('presets', async ({ page }) => {
  await open(page, 'nav-presets');
  await page.getByTestId('preset-item-formal').click();
  await page.waitForTimeout(200);
  await page.screenshot({ path: '../docs/images/presets.png' });
});

test('connections', async ({ page }) => {
  await open(page, 'nav-connections');
  await page.screenshot({ path: '../docs/images/connections.png' });
});

test('behavior', async ({ page }) => {
  await open(page, 'nav-behavior');
  await page.screenshot({ path: '../docs/images/behavior.png' });
});

test('models', async ({ page }) => {
  await open(page, 'nav-models');
  await page.screenshot({ path: '../docs/images/models.png' });
});

