'use client';

// Theme (system/dark/light) persisted via the settings store and applied to
// <html> as a `light` class — mirroring the wireframes' pre-paint convention
// (docs/wireframes/*.html inline `<script>` + app.js `applyTheme`), except
// the source of truth is the backend settings store (settings_get/set on the
// 'theme' key) rather than localStorage, so it survives on a fresh machine and
// stays in sync with the General screen's Appearance control (which writes the
// same key). A14 wires this into NavRail's theme-toggle and rehydrates it on
// boot in App.tsx.

import { settingsGet, settingsSet } from './ipc';

export type Theme = 'system' | 'dark' | 'light';

/** Settings key the theme is persisted under (shared with General.tsx). */
export const THEME_SETTINGS_KEY = 'theme';

/** The default when nothing has been persisted yet: follow the OS. */
export const DEFAULT_THEME: Theme = 'system';

function isTheme(value: unknown): value is Theme {
  return value === 'system' || value === 'dark' || value === 'light';
}

/** Whether the OS is currently asking for a light appearance. */
function prefersLight(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: light)').matches
  );
}

/**
 * Resolves a theme choice to a concrete light/dark boolean: `system` follows
 * the OS preference; `light`/`dark` are absolute.
 */
export function resolveIsLight(theme: Theme): boolean {
  return theme === 'light' || (theme === 'system' && prefersLight());
}

/**
 * The theme you'd switch to when toggling from `theme`. A fixed light/dark
 * theme flips to its opposite; `system` flips based on what it currently
 * resolves to, so one click always visibly changes the appearance.
 */
export function toggledTheme(theme: Theme): Theme {
  return resolveIsLight(theme) ? 'dark' : 'light';
}

/** Applies `theme` to <html> by toggling the `light` class (no persistence). */
export function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('light', resolveIsLight(theme));
}

/**
 * Reads the persisted theme via `settings_get`, falling back to
 * [`DEFAULT_THEME`] when unset, unrecognized, or the backend is unavailable.
 */
export async function loadTheme(): Promise<Theme> {
  try {
    const stored = await settingsGet(THEME_SETTINGS_KEY);
    return isTheme(stored) ? stored : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

/**
 * Applies `theme` to <html> and persists it via `settings_set`. The class is
 * toggled first (so the UI updates immediately even if the write is slow or
 * fails) and a failed persist is swallowed rather than throwing at the click
 * handler.
 */
export async function setTheme(theme: Theme): Promise<void> {
  applyTheme(theme);
  try {
    await settingsSet(THEME_SETTINGS_KEY, theme);
  } catch {
    // Persistence is best-effort; the in-memory/class change already happened.
  }
}
