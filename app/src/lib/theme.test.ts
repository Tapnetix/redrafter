import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { applyTheme, loadTheme, resolveIsLight, setTheme, toggledTheme, THEME_SETTINGS_KEY } from './theme';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

function setPrefersLight(matches: boolean) {
  window.matchMedia = vi.fn().mockReturnValue({ matches }) as unknown as typeof window.matchMedia;
}

describe('resolveIsLight', () => {
  beforeEach(() => setPrefersLight(false));

  it('is light for the "light" theme regardless of OS preference', () => {
    expect(resolveIsLight('light')).toBe(true);
  });

  it('is dark for the "dark" theme regardless of OS preference', () => {
    expect(resolveIsLight('dark')).toBe(false);
  });

  it('follows the OS preference for the "system" theme', () => {
    setPrefersLight(true);
    expect(resolveIsLight('system')).toBe(true);
    setPrefersLight(false);
    expect(resolveIsLight('system')).toBe(false);
  });
});

describe('toggledTheme', () => {
  beforeEach(() => setPrefersLight(false));

  it('flips light to dark', () => {
    expect(toggledTheme('light')).toBe('dark');
  });

  it('flips dark to light', () => {
    expect(toggledTheme('dark')).toBe('light');
  });

  it('flips "system" based on what it currently resolves to', () => {
    setPrefersLight(true);
    expect(toggledTheme('system')).toBe('dark');
    setPrefersLight(false);
    expect(toggledTheme('system')).toBe('light');
  });
});

describe('applyTheme', () => {
  beforeEach(() => {
    setPrefersLight(false);
    document.documentElement.classList.remove('light');
  });

  it('adds the "light" class to <html> for a light theme', () => {
    applyTheme('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });

  it('removes the "light" class from <html> for a dark theme', () => {
    document.documentElement.classList.add('light');
    applyTheme('dark');
    expect(document.documentElement.classList.contains('light')).toBe(false);
  });
});

describe('loadTheme / setTheme', () => {
  beforeEach(() => {
    setPrefersLight(false);
    document.documentElement.classList.remove('light');
    mockedInvoke.mockReset();
  });
  afterEach(() => {
    mockedInvoke.mockReset();
  });

  it('loadTheme reads the persisted theme via settings_get', async () => {
    mockedInvoke.mockResolvedValue('dark');

    await expect(loadTheme()).resolves.toBe('dark');
    expect(mockedInvoke).toHaveBeenCalledWith('settings_get', { key: THEME_SETTINGS_KEY });
  });

  it('loadTheme falls back to "system" when nothing is persisted', async () => {
    mockedInvoke.mockResolvedValue(null);

    await expect(loadTheme()).resolves.toBe('system');
  });

  it('loadTheme falls back to "system" on an unrecognized stored value', async () => {
    mockedInvoke.mockResolvedValue('not-a-theme');

    await expect(loadTheme()).resolves.toBe('system');
  });

  it('loadTheme falls back to "system" when settings_get rejects', async () => {
    mockedInvoke.mockRejectedValue(new Error('no backend'));

    await expect(loadTheme()).resolves.toBe('system');
  });

  it('setTheme persists via settings_set and applies the class to <html>', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await setTheme('light');

    expect(mockedInvoke).toHaveBeenCalledWith('settings_set', { key: THEME_SETTINGS_KEY, value: 'light' });
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });
});
