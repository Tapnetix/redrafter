'use client';

// General settings screen (wireframes/index.html, S20: "General surfaces
// status"). Phase A (A12) built the STATUS surface: permission status, the
// current hotkey, an active-model summary, a menu-bar link, and the theme
// control. C6 (this extension, S34) builds the two controls A12 left
// display-only:
//   - `hotkey-change` opens the rebind capture dialog (`hotkey-modal`):
//     records a combo via keydown, then `hotkey-save` calls `hotkeySet`.
//     A conflict (`result.conflict === true`) shows `hotkey-conflict`
//     rather than closing the dialog or losing the current hotkey --
//     `hotkey.rs`'s `apply_combo` never touches the previous combo on a
//     conflict, so neither does this.
//   - `general-launch-login` toggles launch-at-login, persisted through
//     the same `setLaunchAtLogin`/`launch_at_login` path `Tray.tsx` (B17)
//     already uses -- see `lifecycle.rs`'s module doc, which expects this
//     control to drive the same backend (`tray_set_launch_login`) so the
//     real `tauri-plugin-autostart` call actually fires, rather than a
//     bare `settings_set` that would silently desync from it.
//   - `active-model-link` real navigation: Phase B (Models) territory,
//     still out of scope here.
import { useEffect, useRef, useState } from 'react';
import { getPermissionStatus, hotkeySet, setLaunchAtLogin, settingsGet, settingsSet } from '@/lib/ipc';

const HOTKEY_SETTINGS_KEY = 'hotkey_combo';
const THEME_SETTINGS_KEY = 'theme';
const LAUNCH_LOGIN_SETTINGS_KEY = 'launch_at_login';
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

/** Modifier keys ignored on their own -- capture waits for the following
 * non-modifier key before producing a combo. */
const MODIFIER_KEYS = new Set(['Control', 'Meta', 'Alt', 'Shift']);

/** The subset of `KeyboardEvent`/React's `KeyboardEvent` that
 * `comboFromKeyboardEvent` needs -- kept narrow so it's easy to call with a
 * plain object from a test. */
interface HotkeyCaptureEvent {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/**
 * Builds a combo string (e.g. `"Ctrl+Alt+S"`) from a keydown event, in the
 * long-name form `hotkeySet`/the backend expect (not the display glyphs
 * `formatHotkey` produces). Returns `null` for a bare modifier keypress,
 * since capture waits for the key it's held with.
 */
export function comboFromKeyboardEvent(e: HotkeyCaptureEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Cmd');
  const key = e.key === ' ' ? 'Space' : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  parts.push(key);
  return parts.join('+');
}

export default function General() {
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(null);
  const [hotkeyCombo, setHotkeyCombo] = useState<string>(DEFAULT_HOTKEY_COMBO);
  const [theme, setTheme] = useState<Theme>('system');
  const [launchAtLogin, setLaunchAtLoginState] = useState(true);

  const [hotkeyDialogOpen, setHotkeyDialogOpen] = useState(false);
  const [capturedCombo, setCapturedCombo] = useState<string | null>(null);
  const [hotkeyConflict, setHotkeyConflict] = useState(false);
  const [hotkeySaving, setHotkeySaving] = useState(false);
  const captureRef = useRef<HTMLDivElement | null>(null);

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
    settingsGet(LAUNCH_LOGIN_SETTINGS_KEY)
      .then((value) => setLaunchAtLoginState(value !== 'false'))
      .catch(() => setLaunchAtLoginState(true));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (hotkeyDialogOpen) {
      captureRef.current?.focus();
    }
  }, [hotkeyDialogOpen]);

  const chooseTheme = (next: Theme) => {
    setTheme(next);
    settingsSet(THEME_SETTINGS_KEY, next).catch(() => {});
  };

  const toggleLaunchAtLogin = () => {
    const next = !launchAtLogin;
    setLaunchAtLoginState(next);
    setLaunchAtLogin(next).catch(() => setLaunchAtLoginState(!next));
  };

  const openHotkeyDialog = () => {
    setCapturedCombo(null);
    setHotkeyConflict(false);
    setHotkeyDialogOpen(true);
  };

  const closeHotkeyDialog = () => {
    setHotkeyDialogOpen(false);
    setCapturedCombo(null);
    setHotkeyConflict(false);
  };

  const captureHotkey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      closeHotkeyDialog();
      return;
    }
    const combo = comboFromKeyboardEvent(e);
    if (!combo) return;
    e.preventDefault();
    setCapturedCombo(combo);
    setHotkeyConflict(false);
  };

  const saveHotkey = () => {
    if (!capturedCombo) return;
    setHotkeySaving(true);
    hotkeySet(capturedCombo)
      .then((result) => {
        setHotkeySaving(false);
        if (result.ok && !result.conflict) {
          setHotkeyCombo(capturedCombo);
          closeHotkeyDialog();
        } else {
          setHotkeyConflict(true);
        }
      })
      .catch(() => {
        setHotkeySaving(false);
        setHotkeyConflict(true);
      });
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
              <button className="btn btn--ghost btn--sm" data-testid="hotkey-change" onClick={openHotkeyDialog}>
                Change
              </button>
            </div>
          </div>
          <div className="opt">
            <div className="opt__main">
              <div className="opt__name">Start redrafter at login</div>
              <div className="opt__desc">Launch the menu-bar app automatically when you sign in.</div>
            </div>
            <div className="opt__ctrl">
              <button
                className="switch"
                role="switch"
                aria-checked={launchAtLogin}
                aria-label="Start at login"
                data-testid="general-launch-login"
                onClick={toggleLaunchAtLogin}
              >
                <span className="track" aria-hidden="true" />
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

      {hotkeyDialogOpen && (
        <div
          className="modal-back"
          style={{ display: 'grid' }}
          role="dialog"
          aria-modal="true"
          aria-labelledby="hotkey-modal-title"
          data-testid="hotkey-modal"
        >
          <div className="modal" style={{ width: 480 }}>
            <h2 className="modal__title" id="hotkey-modal-title">
              Set global hotkey
            </h2>
            <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: '0 0 14px' }}>
              Press a key combination now, or Esc to cancel. Include a modifier (⌘ ⌃ ⌥ ⇧).
            </p>
            <div
              ref={captureRef}
              tabIndex={0}
              role="textbox"
              aria-label="Press the new hotkey combination"
              className="mono"
              data-testid="hotkey-capture"
              onKeyDown={captureHotkey}
              style={{
                textAlign: 'center',
                padding: 18,
                border: '1px solid var(--border)',
                borderRadius: 'var(--radius)',
                background: 'var(--bg)',
              }}
            >
              {capturedCombo ? formatHotkey(capturedCombo) : 'Press keys…'}
            </div>
            {hotkeyConflict && (
              <div data-testid="hotkey-conflict" role="alert" style={{ marginTop: 12 }}>
                <span className="mono">{capturedCombo ? formatHotkey(capturedCombo) : ''}</span> is already used by
                another shortcut. Try a different combination.
              </div>
            )}
            <div className="modal__foot">
              <button className="btn" data-testid="hotkey-cancel" onClick={closeHotkeyDialog}>
                Cancel
              </button>
              <button
                className="btn btn--primary"
                data-testid="hotkey-save"
                disabled={!capturedCombo || hotkeySaving}
                onClick={saveHotkey}
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}

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
