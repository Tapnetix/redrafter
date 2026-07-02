'use client';

// The hotkey capture overlay (wireframes/capture.html, controls/capture.json).
// This task (A9) builds the DEFAULT blind-inject loop (S1): the panel opens
// over the captured selection, `capture-refine` runs the single `refine`
// backend call (capture -> prompt -> model -> inject, all server-side — the
// frontend never calls `inject_text` for this path), and the Done state
// shows the refined result with a working restore control (S12, A10 only
// adds the spec). `refine` failures for a missing active model (S19, A11)
// and a runtime-revoked Accessibility permission (S36, A13) are also handled
// here since those tasks depend on this component without permission to
// modify it.
//
// Out of scope here (Phase B, per B6): live command-parse highlighting,
// the review-and-confirm/edit states, and error/retry with fallback. Those
// extend this file's state machine rather than forking a new component.
//
// The embedded menu-bar tray preview (`tray-*` testids) is DISPLAY-ONLY per
// the task's carve-out: rendered for layout fidelity, with `tray-quit` wired
// to `tray_quit` as explicitly allowed. `tray-model-*`/`tray-fav-*` are inert
// — live model switching is `Tray.tsx` (B9/B23).

import { useCallback, useState } from 'react';
import {
  injectText,
  NO_ACTIVE_MODEL_ERROR,
  PERMISSION_DENIED_ERROR,
  permissionOpenSettings,
  refine,
  restoreOriginal,
  trayQuit,
  type RefineOutcome,
} from '@/lib/ipc';

type PanelState = 'idle' | 'refining' | 'done' | 'no-model' | 'permission';

function errorMessage(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

export interface CaptureProps {
  /** Called when the user dismisses the panel (Esc) without refining. */
  onDismiss?: () => void;
}

export default function Capture({ onDismiss }: CaptureProps) {
  const [state, setState] = useState<PanelState>('idle');
  const [outcome, setOutcome] = useState<RefineOutcome | null>(null);
  const [restored, setRestored] = useState(false);
  const [trayOpen, setTrayOpen] = useState(false);
  const [trayModelOpen, setTrayModelOpen] = useState(false);

  const handleRefine = useCallback(async () => {
    setState('refining');
    try {
      const result = await refine();
      setOutcome(result);
      setRestored(false);
      setState('done');
    } catch (err) {
      const message = errorMessage(err);
      if (message === NO_ACTIVE_MODEL_ERROR) {
        setState('no-model');
      } else if (message === PERMISSION_DENIED_ERROR) {
        setState('permission');
      } else {
        // Generic failures (fallback/retry) are Phase B (B6) — leave the
        // user back at the input state with their text untouched.
        setState('idle');
      }
    }
  }, []);

  const handleRestore = useCallback(async () => {
    try {
      const original = await restoreOriginal();
      await injectText(original);
      setRestored(true);
    } catch {
      // Restore can fail if e.g. Accessibility was revoked mid-session; avoid
      // an unhandled rejection. Richer error/retry UX is Phase B (B6).
    }
  }, []);

  const handleOpenSettings = useCallback(() => {
    void permissionOpenSettings();
  }, []);

  const handleQuit = useCallback(() => {
    void trayQuit();
  }, []);

  return (
    <div className="capture-scrim" data-testid="capture-desk">
      <button
        className="btn btn--ghost"
        data-testid="capture-dismiss"
        aria-label="Dismiss"
        style={{ position: 'fixed', top: 14, left: 20 }}
        onClick={onDismiss}
      >
        Esc
      </button>

      {/* Embedded tray preview — DISPLAY ONLY, see file header carve-out. */}
      <button
        className="btn btn--sm"
        data-testid="tray-btn"
        aria-haspopup="true"
        aria-expanded={trayOpen}
        style={{ position: 'fixed', top: 14, right: 20 }}
        onClick={() => setTrayOpen((v) => !v)}
      >
        <span className="status-dot green" aria-hidden="true" /> redrafter
      </button>
      {trayOpen && (
        <div data-testid="tray" role="menu" aria-label="redrafter tray" style={{ position: 'fixed', top: 48, right: 20 }}>
          <button
            className="menu__item"
            role="menuitem"
            data-testid="tray-active-model"
            aria-expanded={trayModelOpen}
            aria-controls="capture-tray-model-region"
            onClick={() => setTrayModelOpen((v) => !v)}
          >
            Active model
          </button>
          {trayModelOpen && (
            <div id="capture-tray-model-region" data-testid="tray-model-list">
              <button className="menu__item" role="menuitemradio" data-testid="tray-fav-opus" data-model-label="claude-opus-4-6">
                claude-opus-4-6
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-fav-sonnet" data-model-label="claude-sonnet-4-6">
                claude-sonnet-4-6
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-fav-qwen" data-model-label="qwen3:8b">
                qwen3:8b
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-opus" data-model-label="claude-opus-4-6">
                claude-opus-4-6
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-sonnet" data-model-label="claude-sonnet-4-6">
                claude-sonnet-4-6
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-gpt" data-model-label="gpt-5.1">
                gpt-5.1
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-gemini" data-model-label="gemini-1.5-flash">
                gemini-1.5-flash
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-qwen" data-model-label="qwen3:8b">
                qwen3:8b
              </button>
              <button className="menu__item" role="menuitemradio" data-testid="tray-model-llama" data-model-label="llama3.1:8b">
                llama3.1:8b
              </button>
            </div>
          )}
          <span className="menu__item" data-testid="tray-manage-models">
            Manage models…
          </span>
          <span className="menu__item" data-testid="view-tray">
            Full tray menu ↗
          </span>
          <span className="menu__item" data-testid="tray-settings">
            Settings…
          </span>
          <span className="menu__item" data-testid="tray-history">
            History…
          </span>
          <button className="menu__item" role="menuitem" data-testid="tray-quit" onClick={handleQuit}>
            Quit redrafter
          </button>
        </div>
      )}

      <div className="cap" role="dialog" aria-modal="true" aria-label="redrafter capture" data-testid="capture-panel">
        <div className="cap__top">
          <span className="cap__badge" aria-hidden="true">
            R
          </span>
          <h1 style={{ fontSize: 15, fontWeight: 600, margin: 0 }}>Refine selection</h1>
          <span className="cap__model chip mono" data-testid="capture-active-model">
            {outcome ? outcome.model : 'No model selected'}
          </span>
        </div>

        {(state === 'idle' || state === 'refining') && (
          <div>
            <div className="cap__body" data-testid="capture-input">
              <div
                className="cap__editor"
                role="group"
                aria-readonly="true"
                aria-label="Captured selection"
                tabIndex={0}
                data-testid="capture-editor"
              >
                Your captured selection will appear here.
              </div>
              <div className="cap__parse" data-testid="capture-preview" aria-live="polite">
                <span className="lbl">Parsed</span>
                <span className="chip mono">default direction</span>
              </div>
            </div>
            <div className="cap__bar">
              <span className="hint">
                <kbd>⌃⌥R</kbd> invoke · <kbd>Enter</kbd> refine · <kbd>Esc</kbd> cancel
              </span>
              <span className="spacer" />
              <button
                className="btn btn--primary"
                data-testid="capture-refine"
                onClick={handleRefine}
                disabled={state === 'refining'}
              >
                {state === 'refining' ? 'Refining…' : 'Refine'}
              </button>
            </div>
          </div>
        )}

        {state === 'done' && outcome && (
          <section className="cap__body" aria-label="Done" data-testid="capture-done" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="sec__title" style={{ fontSize: 'var(--fs-small)', marginBottom: 8 }}>
              Done · replaced
            </div>
            <div className="diff-block">
              <span className="lbl after">Refined · pasted in place</span>
              <div className="diff">
                <span className="diff__refined">{outcome.refined}</span>
              </div>
            </div>
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="hint" style={{ color: 'var(--refined)' }}>
                {restored ? 'Restored original' : (
                  <>
                    Replaced · undo with <kbd>⌘Z</kbd>
                  </>
                )}
              </span>
              <span className="spacer" />
              <button className="btn" data-testid="capture-restore" onClick={handleRestore} disabled={restored}>
                Restore original
              </button>
            </div>
          </section>
        )}

        {state === 'no-model' && (
          <section className="cap__body" aria-label="No active model" data-testid="capture-no-model" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="sec__title" style={{ fontSize: 'var(--fs-small)', marginBottom: 8 }}>
              No active model
            </div>
            <p className="muted tiny" style={{ margin: 0 }}>
              Choose an active model before refining. Your text is untouched.
            </p>
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="spacer" />
              <button className="btn btn--primary" data-testid="capture-no-model-cta">
                Choose a model
              </button>
            </div>
          </section>
        )}

        {state === 'permission' && (
          <section
            className="cap__body"
            aria-label="Permission needed"
            data-testid="capture-permission"
            style={{ borderTop: '1px solid var(--border)' }}
          >
            <div className="sec__title" style={{ fontSize: 'var(--fs-small)', marginBottom: 8 }}>
              Permission needed
            </div>
            <div style={{ fontWeight: 600, color: 'var(--warning)' }}>Accessibility permission needed</div>
            <p className="muted tiny" style={{ margin: '4px 0 0' }}>
              redrafter can no longer read your selection. Re-grant Accessibility to keep refining.
            </p>
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="spacer" />
              <button className="btn btn--primary" data-testid="capture-perm-open-settings" onClick={handleOpenSettings}>
                Open System Settings
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
