'use client';

// The review panel (`review.rs`): shown in its own window when Behavior's
// inject mode is "Review & confirm", so a refine you asked to check first is
// actually presented rather than silently dropped.
//
// Accept goes through `review_accept` rather than `inject_text` directly: this
// window has focus while you are reading, so the backend has to hide it and
// wait for the window server to hand focus back before injecting, or the
// refined text pastes into this panel instead of the app you came from. That
// ordering lives in Rust so it cannot race.

import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { reviewAccept, reviewDiscard, reviewPending, type RefineOutcome } from '@/lib/ipc';

export default function ReviewPage() {
  const [pending, setPending] = useState<RefineOutcome | null>(null);
  const [draft, setDraft] = useState('');
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const load = useCallback(async () => {
    try {
      const outcome = await reviewPending();
      setPending(outcome ?? null);
      setDraft(outcome?.refined ?? '');
      setEditing(false);
      setError('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // A second refine while the panel is open must replace the draft, not leave
  // the previous one on screen for the user to accept by mistake.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      const p = listen('review:pending', () => void load());
      if (p && typeof p.then === 'function') {
        p.then((un) => (disposed ? un() : (unlisten = un))).catch(() => {});
      }
    } catch {
      // no event system (unit tests / non-Tauri context)
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [load]);

  const accept = useCallback(async () => {
    setBusy(true);
    setError('');
    try {
      await reviewAccept(draft);
    } catch (err) {
      // Injection can genuinely fail (permission revoked, no focused field).
      // Say so and keep the draft on screen rather than losing it.
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }, [draft]);

  const discard = useCallback(async () => {
    setBusy(true);
    try {
      await reviewDiscard();
    } finally {
      setBusy(false);
    }
  }, []);

  // Esc discards, Cmd/Ctrl+Enter accepts — the panel is modal to the flow, so
  // it should be dismissible without reaching for the mouse.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        void discard();
      } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        void accept();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [accept, discard]);

  return (
    <div className="cap" data-testid="review-panel" style={{ position: 'fixed', inset: 0, display: 'flex', flexDirection: 'column', margin: 0, borderRadius: 0 }}>
      <div className="cap__top" data-tauri-drag-region>
        <img className="cap__badge" src="/logo.png" alt="" width={28} height={28} draggable={false} />
        <strong style={{ fontSize: 'var(--fs-small)' }}>Review before inserting</strong>
        {pending && (
          <span className="chip cap__model" data-testid="review-model">
            {pending.model}
          </span>
        )}
      </div>

      <div className="cap__body" style={{ flex: 1, overflowY: 'auto' }}>
        {!pending && (
          <p className="muted" data-testid="review-empty" style={{ margin: 0 }}>
            Nothing is waiting for review.
          </p>
        )}

        {pending && (
          <>
            <p className="muted tiny" style={{ margin: '0 0 6px' }}>Your original</p>
            <div
              className="cap__editor"
              data-testid="review-original"
              style={{ color: 'var(--text-tertiary)', marginBottom: 14, whiteSpace: 'pre-wrap' }}
            >
              {pending.original}
            </div>

            <p className="muted tiny" style={{ margin: '0 0 6px' }}>
              Refined {editing ? '— edit before inserting' : ''}
            </p>
            {editing ? (
              <textarea
                className="cap__editor"
                data-testid="review-edit-field"
                value={draft}
                autoFocus
                onChange={(e) => setDraft(e.target.value)}
                style={{ width: '100%', minHeight: 120, resize: 'vertical', whiteSpace: 'pre-wrap' }}
              />
            ) : (
              <div className="cap__editor" data-testid="review-refined" style={{ whiteSpace: 'pre-wrap' }}>
                {draft}
              </div>
            )}
          </>
        )}

        {error && (
          <p className="tiny" role="alert" data-testid="review-error" style={{ margin: '10px 0 0', color: 'var(--danger, #d0433b)' }}>
            {error}
          </p>
        )}
      </div>

      <div className="cap__actions" style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 16px', borderTop: '1px solid var(--border)' }}>
        <span className="muted tiny">
          <kbd>Esc</kbd> discard · <kbd>⌘↵</kbd> insert
        </span>
        <div style={{ flex: 1 }} />
        <button className="btn btn--ghost btn--sm" data-testid="review-discard" onClick={() => void discard()} disabled={busy}>
          Discard
        </button>
        <button
          className="btn btn--sm"
          data-testid="review-edit"
          onClick={() => setEditing((e) => !e)}
          disabled={busy || !pending}
        >
          {editing ? 'Done editing' : 'Edit'}
        </button>
        <button
          className="btn btn--primary btn--sm"
          data-testid="review-accept"
          onClick={() => void accept()}
          disabled={busy || !pending}
        >
          {busy ? 'Inserting…' : 'Insert'}
        </button>
      </div>
    </div>
  );
}
