'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** Mirrors `PermissionStatus` from src-tauri/src/permission.rs. */
interface PermissionStatus {
  granted: boolean;
}

/** How often to re-poll `permission_status` while ungranted. macOS doesn't
 * push a change notification when the user grants Accessibility in System
 * Settings, so the gate polls until it sees `granted: true`. */
const POLL_INTERVAL_MS = 1000;

export interface OnboardingProps {
  /** Called once the user activates Continue after Accessibility is
   * granted. The caller (app shell / router) decides where "first-run"
   * actually navigates to; this screen only owns the gate itself. */
  onContinue?: () => void;
}

export default function Onboarding({ onContinue }: OnboardingProps) {
  const [granted, setGranted] = useState(false);
  // `checking` drives the Re-check button's feedback; `checkedNotGranted` shows
  // an explicit "still not granted" note so a manual re-check that finds no
  // change isn't silent (the previous button gave no indication at all).
  const [checking, setChecking] = useState(false);
  const [checkedNotGranted, setCheckedNotGranted] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const checkStatus = useCallback(async (manual = false) => {
    if (manual) setChecking(true);
    try {
      const status = await invoke<PermissionStatus>('permission_status');
      setGranted(status.granted);
      if (manual) setCheckedNotGranted(!status.granted);
    } finally {
      if (manual) setChecking(false);
    }
  }, []);

  // Initial check on mount.
  useEffect(() => {
    checkStatus();
  }, [checkStatus]);

  // Re-check automatically until granted. macOS doesn't push a change event
  // when Accessibility is toggled, so poll `permission_status` (which reads
  // AXIsProcessTrusted) until it flips to granted. A manual "Re-check" button
  // and troubleshooting (Applications-folder / remove-and-re-add) cover the
  // cases where the OS keeps reporting false after the toggle.
  useEffect(() => {
    if (granted) {
      return;
    }
    pollRef.current = setInterval(checkStatus, POLL_INTERVAL_MS);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [granted, checkStatus]);

  const handleOpenSettings = useCallback(() => {
    void invoke('permission_open_settings');
  }, []);

  const handleContinue = useCallback(() => {
    if (!granted) return;
    onContinue?.();
  }, [granted, onContinue]);

  return (
    <div className="setup">
      <div className="setup__card">
        <img className="setup__logo" src="/logo.png" alt="" width={46} height={46} draggable={false} />
        <p className="eyebrow">First run</p>
        <h1>One permission to redraft anywhere</h1>
        <p className="muted" style={{ fontSize: 16, margin: '0 0 20px', maxWidth: '52ch' }}>
          redrafter works inside every app on your Mac. To do that it needs
          one thing from macOS — the <strong>Accessibility</strong>{' '}
          permission.
        </p>

        <section aria-labelledby="why-title">
          <h2 id="why-title">Why Accessibility?</h2>
          <p className="muted" style={{ fontSize: 'var(--fs-small)', margin: '0 0 4px' }}>
            The permission lets redrafter do exactly two things when you
            press the hotkey:
          </p>
          <div className="why-grid">
            <div className="why">
              <strong>Read your selection.</strong>
              <br />
              <span className="muted">
                Copies the highlighted text so the model has something to
                refine.
              </span>
            </div>
            <div className="why">
              <strong>Paste it back.</strong>
              <br />
              <span className="muted">
                Replaces your selection with the refined result, in place.
              </span>
            </div>
          </div>
          <p className="muted tiny" style={{ margin: '0 0 6px' }}>
            redrafter never reads your screen in the background — it runs
            only on the text you select, only when you invoke it.
          </p>
          <p className="muted tiny" data-testid="perm-capture-tech-note" style={{ margin: '0 0 20px' }}>
            redrafter reads your selection via the Accessibility API, with
            clipboard-based capture as a fallback.
          </p>
        </section>

        <section aria-labelledby="status-title">
          <h2 id="status-title">Status</h2>
          <div className="grp">
            <div className="opt" data-testid="perm-status" data-granted={granted ? 'true' : 'false'}>
              <span className={`status-dot ${granted ? 'green' : 'red'}`} aria-hidden="true" />
              <div className="opt__main">
                <div className="opt__name">
                  Accessibility — <span>{granted ? 'Granted' : 'Not granted'}</span>
                </div>
                <div className="opt__desc">
                  redrafter needs this to read your selection and paste back.
                </div>
              </div>
              <div className="opt__ctrl" style={{ display: 'flex', gap: 8 }}>
                <button
                  className="btn btn--primary"
                  data-testid="perm-open-settings"
                  onClick={handleOpenSettings}
                >
                  Open System Settings
                </button>
                <button
                  className="btn btn--ghost"
                  data-testid="perm-recheck"
                  onClick={() => void checkStatus(true)}
                  disabled={checking}
                >
                  {checking ? 'Checking…' : 'Re-check'}
                </button>
              </div>
            </div>
          </div>
          {checkedNotGranted && !granted && (
            <p
              className="tiny"
              data-testid="perm-recheck-result"
              role="status"
              style={{ margin: '8px 0 0', color: 'var(--danger, #d0433b)' }}
            >
              Checked — macOS still reports it as not granted. See the steps below.
            </p>
          )}
          <ol className="muted tiny" style={{ margin: '10px 0 0', paddingLeft: 18, lineHeight: 1.6 }}>
            <li>Open System Settings → Privacy &amp; Security → Accessibility.</li>
            <li>Turn <strong>redrafter</strong> on.</li>
            <li>
              Come back here — redrafter re-checks every second, or press{' '}
              <strong>Re-check</strong>.
            </li>
          </ol>

          <details data-testid="perm-troubleshoot" style={{ marginTop: 12 }}>
            <summary className="muted tiny" style={{ cursor: 'pointer' }}>
              Turned it on but it still says “Not granted”?
            </summary>
            <p className="muted tiny" style={{ margin: '8px 0 0', lineHeight: 1.6 }}>
              macOS ties the permission to the app’s exact location, so:
            </p>
            <ul className="muted tiny" style={{ margin: '6px 0 0', paddingLeft: 18, lineHeight: 1.6 }}>
              <li>
                Make sure redrafter is in your <strong>Applications</strong> folder —
                running it straight from the download or the disk image makes macOS
                launch it from a temporary path each time, so the permission never
                sticks. Move it to Applications, then reopen it.
              </li>
              <li>
                Still stuck? In the Accessibility list, select redrafter, press the{' '}
                <strong>–</strong> button to remove it, then add it back and turn it on.
              </li>
            </ul>
          </details>
        </section>

        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 24 }}>
          <span className="muted tiny">Continue is enabled once Accessibility is granted.</span>
          <div style={{ flex: 1 }} />
          <button
            className="btn btn--ghost"
            data-testid="perm-continue"
            aria-disabled={!granted}
            disabled={!granted}
            onClick={handleContinue}
          >
            Continue → add a provider
          </button>
        </div>
      </div>
    </div>
  );
}
