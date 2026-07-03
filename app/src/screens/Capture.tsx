'use client';

// The hotkey capture overlay (wireframes/capture.html, controls/capture.json).
// A9 built the DEFAULT blind-inject loop (S1): the panel opens over the
// captured selection, `capture-refine` runs the single `refine` backend call
// (capture -> prompt -> model -> inject, all server-side — the frontend
// never calls `inject_text` for this path), and the Done state shows the
// refined result with a working restore control (S12, A10 only adds the
// spec). `refine` failures for a missing active model (S19, A11) and a
// runtime-revoked Accessibility permission (S36, A13) are also handled here
// since those tasks depend on this component without permission to modify
// it.
//
// B6 extends that state machine with the Phase-B states `controls/
// capture.json`/`wireframes/capture.html` declare:
//   - Live command-parse preview (`capture-preview`): a pure, read-only
//     mirror of the backend `command_parser` (`lib/command-preview.ts`)
//     over `capturedText`. Phase A/B don't expose a "peek at the current
//     selection" backend query yet (the blind pipeline only surfaces the
//     *result* of a refine, not the raw selection beforehand) — `capturedText`
//     is the seam a future capture-peek command can feed without further
//     changes here; until then it defaults to the same placeholder Phase A
//     showed.
//   - Review-and-confirm (`capture-review`/`capture-edit-state`): when the
//     configured inject mode is `Review`, `refine`'s result carries
//     `status: 'pending_review'` (B5's `RefineFlow`) instead of already
//     being injected. Per `controls/capture.json`, Accept/Edit-accept paste
//     the (possibly user-edited) text via the existing `inject_text`, and
//     Discard leaves the original untouched via the existing `cancel_refine`
//     — no new backend commands needed for this surface.
//   - Error/retry with fallback (`capture-error`): a generic refine failure
//     (previously silently returned to idle) now surfaces the failure with a
//     Retry control (`capture-retry`, reusing `refine`) and, when the
//     rejection carries a `fallbackModels` list, an indication a fallback
//     chain is configured.
//
// The embedded menu-bar tray preview (`tray-*` testids) is DISPLAY-ONLY per
// A9's carve-out: rendered for layout fidelity, with `tray-quit` wired to
// `tray_quit` as explicitly allowed. `tray-model-*`/`tray-fav-*` are inert
// — live model switching is `Tray.tsx` (B9/B23).

import { useCallback, useState } from 'react';
import {
  cancelRefine,
  injectText,
  NO_ACTIVE_MODEL_ERROR,
  PERMISSION_DENIED_ERROR,
  permissionOpenSettings,
  refine,
  restoreOriginal,
  trayQuit,
  type RefineOutcome,
} from '@/lib/ipc';
import { parseCommandPreview } from '@/lib/command-preview';

type PanelState = 'idle' | 'refining' | 'done' | 'review' | 'edit' | 'error' | 'no-model' | 'permission';

/** The captured-selection placeholder shown until a real capture-peek
 * command exists (see the file header note on `capturedText`). */
const DEFAULT_CAPTURED_TEXT = 'Your captured selection will appear here.';

function errorMessage(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

/** A generic refine failure, optionally carrying the fallback chain the
 * backend was configured to try (B6's error state shows it when present).
 * Falls back to `errorMessage` for a plain string/Error rejection. */
function parseRefineError(err: unknown): { message: string; fallbackModels?: string[] } {
  if (typeof err === 'object' && err !== null && 'message' in err) {
    const candidate = err as { message?: unknown; fallbackModels?: unknown };
    const fallbackModels = Array.isArray(candidate.fallbackModels)
      ? candidate.fallbackModels.filter((m): m is string => typeof m === 'string')
      : undefined;
    return {
      message: typeof candidate.message === 'string' ? candidate.message : errorMessage(err),
      fallbackModels: fallbackModels && fallbackModels.length > 0 ? fallbackModels : undefined,
    };
  }
  return { message: errorMessage(err) };
}

export interface CaptureProps {
  /** Called when the user dismisses the panel (Esc) without refining. */
  onDismiss?: () => void;
  /**
   * The captured selection text to display in the editor and run through
   * the live command-parse preview. See the file header note: Phase A/B
   * have no backend query for the real OS selection ahead of a refine call,
   * so this defaults to a placeholder.
   */
  capturedText?: string;
}

export default function Capture({ onDismiss, capturedText = DEFAULT_CAPTURED_TEXT }: CaptureProps) {
  const [state, setState] = useState<PanelState>('idle');
  const [outcome, setOutcome] = useState<RefineOutcome | null>(null);
  const [editText, setEditText] = useState('');
  const [refineError, setRefineError] = useState<{ message: string; fallbackModels?: string[] } | null>(null);
  const [restored, setRestored] = useState(false);
  const [trayOpen, setTrayOpen] = useState(false);
  const [trayModelOpen, setTrayModelOpen] = useState(false);

  const preview = parseCommandPreview(capturedText);
  // No `/rd /m /q /lang /<preset>` tag was recognized at all — the parser's
  // no-tags case (`message` is just the trimmed selection verbatim).
  const noTagsFound =
    !preview.direction && !preview.quote && !preview.lang && !preview.preset && preview.message === capturedText.trim();

  const handleRefine = useCallback(async () => {
    setState('refining');
    try {
      const result = await refine();
      setOutcome(result);
      setRestored(false);
      if (result.status === 'pending_review') {
        setEditText(result.refined);
        setState('review');
      } else {
        setState('done');
      }
    } catch (err) {
      const message = errorMessage(err);
      if (message === NO_ACTIVE_MODEL_ERROR) {
        setState('no-model');
      } else if (message === PERMISSION_DENIED_ERROR) {
        setState('permission');
      } else {
        setRefineError(parseRefineError(err));
        setState('error');
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
      // an unhandled rejection.
    }
  }, []);

  const handleAccept = useCallback(async () => {
    if (!outcome) return;
    try {
      await injectText(outcome.refined);
      setRestored(false);
      setState('done');
    } catch {
      // Leave the result pending review so the user can retry accept/edit/discard.
    }
  }, [outcome]);

  const handleEditStart = useCallback(() => {
    setEditText(outcome?.refined ?? '');
    setState('edit');
  }, [outcome]);

  const handleEditCancel = useCallback(() => {
    setState('review');
  }, []);

  const handleEditAccept = useCallback(async () => {
    try {
      await injectText(editText);
      setOutcome((prev) => (prev ? { ...prev, refined: editText } : prev));
      setRestored(false);
      setState('done');
    } catch {
      // Leave the user in the edit state so they can retry.
    }
  }, [editText]);

  const handleDiscard = useCallback(async () => {
    try {
      await cancelRefine();
    } catch {
      // Best-effort — still return to the input state either way, leaving
      // the original selection untouched (nothing was ever injected).
    }
    setOutcome(null);
    setState('idle');
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
                {capturedText}
              </div>
              <div className="cap__parse" data-testid="capture-preview" aria-live="polite">
                <span className="lbl">Parsed</span>
                {noTagsFound ? (
                  <span className="chip mono">default direction</span>
                ) : (
                  <>
                    {preview.direction && (
                      <span className="chip">
                        <span className="tag tag--rd">/rd</span> direction
                      </span>
                    )}
                    {preview.message && (
                      <span className="chip">
                        <span className="tag tag--m">/m</span> your message
                      </span>
                    )}
                    {preview.quote && (
                      <span className="chip">
                        <span className="tag tag--q">/q</span> quote detected
                      </span>
                    )}
                    {preview.preset && (
                      <span className="chip">
                        <span className="tag tag--rd">/{preview.preset}</span> preset
                      </span>
                    )}
                    <span className="chip mono">lang: {preview.lang ?? 'auto'}</span>
                  </>
                )}
              </div>
              {/* Illustrative-only examples of `/lang`/preset syntax — not
                  derived from `capturedText`; mirrors wireframes/capture.html. */}
              <div className="cap__parse" data-testid="capture-preview-lang" aria-live="polite">
                <span className="lbl">e.g.</span>
                <span className="mono tiny muted">
                  …refine my answer <span className="tag tag--lang">/lang de</span>
                </span>
                <span className="chip">
                  <span className="tag tag--lang">/lang de</span> output → German
                </span>
              </div>
              <div className="cap__parse" data-testid="capture-preview-preset">
                <span className="lbl">e.g.</span>
                <span className="mono tiny muted">
                  <span className="tag tag--rd">/formal</span> attached is the report…
                </span>
                <span className="chip">
                  <span className="tag tag--rd">/formal</span> preset · formal rewrite
                </span>
              </div>
              <div
                className="cap__parse"
                data-testid="capture-legend"
                style={{ borderTop: '1px solid var(--border)', marginTop: 6, paddingTop: 10 }}
              >
                <span className="chip">
                  <span className="tag tag--rd">/rd</span> direction
                </span>
                <span className="chip">
                  <span className="tag tag--m">/m</span> message
                </span>
                <span className="chip">
                  <span className="tag tag--q">/q</span> quote
                </span>
                <span className="chip">
                  <span className="tag tag--lang">/lang &lt;code&gt;</span> language
                </span>
                <span className="chip">
                  <span className="mono">/&lt;preset&gt;</span> saved preset
                </span>
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

        {state === 'review' && outcome && (
          <section className="cap__body" aria-label="Review" data-testid="capture-review" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="sec__title" style={{ fontSize: 'var(--fs-small)', marginBottom: 8 }}>
              Review before pasting
            </div>
            <div className="diff-block" style={{ marginBottom: 12 }}>
              <span className="lbl before">Original</span>
              <div className="diff">
                <span className="diff__orig">{outcome.original}</span>
              </div>
            </div>
            <div className="diff-block">
              <span className="lbl after">Refined · review before pasting</span>
              <div className="diff">
                <span className="diff__refined">{outcome.refined}</span>
              </div>
            </div>
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="hint">
                <kbd>Enter</kbd> accept
              </span>
              <span className="spacer" />
              <button className="btn" data-testid="capture-discard" onClick={handleDiscard}>
                Discard
              </button>
              <button className="btn" data-testid="capture-edit" onClick={handleEditStart}>
                Edit
              </button>
              <button className="btn btn--primary" data-testid="capture-accept" onClick={handleAccept}>
                Accept &amp; paste
              </button>
            </div>
          </section>
        )}

        {state === 'edit' && (
          <section className="cap__body" aria-label="Edit" data-testid="capture-edit-state" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="field">
              <label htmlFor="capture-edit-field">Edit the refined text before pasting</label>
              <textarea
                className="textarea"
                id="capture-edit-field"
                data-testid="capture-edit-field"
                rows={4}
                value={editText}
                onChange={(e) => setEditText(e.target.value)}
              />
            </div>
            <p className="muted tiny" style={{ margin: '8px 0 0' }}>
              Your original selection is preserved until you accept.
            </p>
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="hint">
                <kbd>⌘Enter</kbd> accept · <kbd>Esc</kbd> back
              </span>
              <span className="spacer" />
              <button className="btn" data-testid="capture-edit-cancel" onClick={handleEditCancel}>
                Cancel
              </button>
              <button className="btn btn--primary" data-testid="capture-edit-accept" onClick={handleEditAccept}>
                Accept &amp; paste
              </button>
            </div>
          </section>
        )}

        {state === 'error' && refineError && (
          <section className="cap__body" aria-label="Error" data-testid="capture-error" style={{ borderTop: '1px solid var(--border)' }}>
            <div style={{ fontWeight: 600, color: 'var(--orig)' }}>{refineError.message}</div>
            <p className="muted tiny" style={{ margin: '4px 0 0' }}>Your text is untouched.</p>
            {refineError.fallbackModels && refineError.fallbackModels.length > 0 && (
              <p className="muted tiny" style={{ margin: '8px 0 0' }} data-testid="capture-error-fallback">
                Trying fallback models: {refineError.fallbackModels.join(', ')}
              </p>
            )}
            <div className="cap__bar" style={{ padding: 0, marginTop: 10 }}>
              <span className="spacer" />
              <button className="btn btn--primary" data-testid="capture-retry" onClick={handleRefine}>
                Retry
              </button>
            </div>
          </section>
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
