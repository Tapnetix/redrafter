'use client';

// General settings screen (wireframes/index.html, S20: "General surfaces
// status"). Phase A (this task, A12) builds the STATUS surface only:
// permission status, the current hotkey, an active-model summary, a
// menu-bar link, and the theme control. Controls whose full behavior
// belongs to a later phase are rendered as inert display/links here:
//   - `hotkey-change` opens the hotkey rebind dialog: built by C6.
//   - `general-launch-login` toggle: added by C6 (extends this file).
//   - `active-model-link` / `general-tray-link` real navigation: Phase B
//     (Models) / the Tray screen aren't built yet in Phase A.
import { useEffect, useState } from 'react';
import { getPermissionStatus, settingsGet, settingsSet } from '@/lib/ipc';

const HOTKEY_SETTINGS_KEY = 'hotkey_combo';
const THEME_SETTINGS_KEY = 'theme';
const DEFAULT_HOTKEY_COMBO = 'Ctrl+Alt+R';

type Theme = 'system' | 'dark' | 'light';

const MODIFIER_GLYPHS: Record<string, string> = {
  Ctrl: '⌃',
  Control: '⌃',
  Alt: '⌥',
  Option: '⌥',
  Shift: '⇧',
  Cmd: '⌘',
  Command: '⌘',
  Super: '⌘',
  Meta: '⌘',
};

/**
 * Formats a combo string like "Ctrl+Alt+R" as the compact glyph form the
 * wireframe uses ("⌃⌥R"): modifiers become symbols, the trailing key stays
 * as-is, and there are no separators.
 */
export function formatHotkey(combo: string): string {
  const parts = combo.split('+').filter(Boolean);
  return parts.map((part) => MODIFIER_GLYPHS[part] ?? part).join('');
}

export default function General() {
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(null);
  const [hotkeyCombo, setHotkeyCombo] = useState<string>(DEFAULT_HOTKEY_COMBO);
  const [theme, setTheme] = useState<Theme>('system');

  const refreshPermission = () => {
    getPermissionStatus()
      .then((status) => setPermissionGranted(status.granted))
      .catch(() => setPermissionGranted(false));
  };

  useEffect(() => {
    refreshPermission();
    settingsGet(HOTKEY_SETTINGS_KEY)
      .then((value) => setHotkeyCombo(value ?? DEFAULT_HOTKEY_COMBO))
      .catch(() => setHotkeyCombo(DEFAULT_HOTKEY_COMBO));
    settingsGet(THEME_SETTINGS_KEY)
      .then((value) => {
        if (value === 'dark' || value === 'light' || value === 'system') {
          setTheme(value);
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const chooseTheme = (next: Theme) => {
    setTheme(next);
    settingsSet(THEME_SETTINGS_KEY, next).catch(() => {});
  };

  return (
    <div className="settings">
      {/* PERMISSION */}
      <section className="sec" data-testid="general-permission">
        <h2 className="sec__title">Permissions</h2>
        <div className="grp">
          <div className="opt" data-testid="perm-status" data-granted={String(!!permissionGranted)}>
            <span className={`status-dot ${permissionGranted ? 'green' : 'red'}`} aria-hidden="true" />
            <div className="opt__main">
              <div className="opt__name">
                Accessibility{' '}
                <span className={`chip ${permissionGranted ? 'ok' : 'err'}`} style={{ marginLeft: 4 }}>
                  {permissionGranted ? 'Granted ✓' : 'Not granted'}
                </span>
              </div>
              <div className="opt__desc">
                Required so redrafter can read your selection and paste the refined text back in place.
              </div>
            </div>
            <div className="opt__ctrl">
              <button className="btn btn--sm" data-testid="perm-recheck" onClick={refreshPermission}>
                Re-check
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* CAPTURE / HOTKEY */}
      <section className="sec" data-testid="general-hotkey">
        <h2 className="sec__title">Capture</h2>
        <div className="grp">
          <div className="opt">
            <div className="opt__main">
              <div className="opt__name">Global hotkey</div>
              <div className="opt__desc">Press anywhere to refine the current selection.</div>
            </div>
            <div className="opt__ctrl kbd-field">
              <span className="mono" data-testid="hotkey-value">
                {formatHotkey(hotkeyCombo)}
              </span>
              <button className="btn btn--ghost btn--sm" data-testid="hotkey-change">
                Change
              </button>
            </div>
          </div>
          <button className="opt" data-testid="general-tray-link" style={{ color: 'inherit', textAlign: 'left' }}>
            <div className="opt__main">
              <div className="opt__name">Menu-bar icon</div>
              <div className="opt__desc">redrafter lives in your menu bar — see the icon states and dropdown.</div>
            </div>
          </button>
        </div>
      </section>

      {/* ACTIVE MODEL SUMMARY */}
      <section className="sec" data-testid="general-active-model">
        <h2 className="sec__title">Active model</h2>
        <div className="grp" style={{ marginTop: 8 }}>
          <button className="opt" data-testid="active-model-link" style={{ color: 'inherit', textAlign: 'left' }}>
            <span className="status-dot amber" aria-hidden="true" />
            <div className="opt__main">
              <div className="opt__name mono">No model selected</div>
              <div className="opt__desc">Choose a connection and model on the Models screen.</div>
            </div>
          </button>
        </div>
      </section>

      {/* APPEARANCE */}
      <section className="sec" data-testid="general-theme">
        <h2 className="sec__title">Appearance</h2>
        <div className="grp">
          <div className="opt">
            <div className="opt__main">
              <div className="opt__name">Theme</div>
              <div className="opt__desc">Match the OS, or pick a fixed appearance.</div>
            </div>
            <div className="opt__ctrl">
              <div className="segmented" data-testid="setting-theme">
                <button
                  className={theme === 'system' ? 'active' : ''}
                  data-testid="theme-system"
                  onClick={() => chooseTheme('system')}
                >
                  System
                </button>
                <button
                  className={theme === 'dark' ? 'active' : ''}
                  data-testid="theme-dark"
                  onClick={() => chooseTheme('dark')}
                >
                  Dark
                </button>
                <button
                  className={theme === 'light' ? 'active' : ''}
                  data-testid="theme-light"
                  onClick={() => chooseTheme('light')}
                >
                  Light
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
