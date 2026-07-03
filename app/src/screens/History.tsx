'use client';

// History settings screen (wireframes/history.html, controls/history.json).
// C4 built the past-refinements list plus its core actions: restoring a
// past entry's original text back into the focused app, and re-refining a
// past entry's original to produce a fresh result. C12-C15 (this file's
// remaining state/markup) add the rest of the screen's affordances per
// `wireframes/history.html`/`controls/history.json`: a client-side search
// box (C14), the full-detail dialog (C13), a per-entry/detail copy control
// (C12), and a clear-all control with confirmation (C15). The backend this
// calls through `@/lib/ipc` (`app/src-tauri/src/history.rs`) is C4's (list/
// get/restore/rerefine) plus this file's own `history_copy`/`history_clear`
// additions; C17 registers every one of those commands in the Tauri invoke
// handler and wires `history_append` into every successful `refine` — until
// then, calling them against a real backend rejects (mirrors `Models.tsx`'s
// note on B23).
import { useEffect, useMemo, useState } from 'react';
import {
  historyClear,
  historyCopy,
  historyList,
  historyReRefine,
  historyRestore,
  type HistoryEntry,
} from '@/lib/ipc';

function formatWhen(createdAt: number): string {
  try {
    return new Date(createdAt).toLocaleString();
  } catch {
    return '';
  }
}

/** Whether `entry` matches the (already-trimmed, lowercased) search `query`
 * — checked against every field a user might search by: the original text,
 * the refined text, the model, and the inline command trigger, if any. */
function matchesQuery(entry: HistoryEntry, query: string): boolean {
  if (!query) return true;
  const haystack = [entry.original, entry.refined, entry.model, entry.command ?? '']
    .join('\n')
    .toLowerCase();
  return haystack.includes(query);
}

/** Writes `text` to the OS clipboard via the browser Clipboard API. Kept as
 * its own helper so both the per-row and detail-dialog copy controls share
 * the same fallback (older/permission-restricted environments may lack
 * `navigator.clipboard`, in which case this resolves without throwing —
 * the `history_copy` call itself still succeeded either way). */
async function writeClipboard(text: string): Promise<void> {
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
  }
}

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [message, setMessage] = useState('');
  const [query, setQuery] = useState('');
  const [detailEntry, setDetailEntry] = useState<HistoryEntry | null>(null);
  const [clearOpen, setClearOpen] = useState(false);

  const load = () => {
    historyList()
      .then(setEntries)
      .catch(() => setEntries([]));
  };

  useEffect(() => {
    load();
  }, []);

  const trimmedQuery = query.trim().toLowerCase();
  const filtered = useMemo(
    () => entries.filter((entry) => matchesQuery(entry, trimmedQuery)),
    [entries, trimmedQuery],
  );

  async function restore(entry: HistoryEntry) {
    try {
      await historyRestore(entry.id);
      setMessage('Original restored');
    } catch {
      setMessage('Restore failed');
    }
  }

  async function reRefine(entry: HistoryEntry) {
    try {
      const updated = await historyReRefine({ id: entry.id });
      setEntries((prev) => [updated, ...prev]);
      setMessage('Re-refined');
    } catch {
      setMessage('Re-refine failed');
    }
  }

  async function copy(entry: HistoryEntry) {
    try {
      const copied = await historyCopy(entry.id);
      await writeClipboard(copied.refined);
      setMessage('Copied refined text');
    } catch {
      setMessage('Copy failed');
    }
  }

  async function clearAll() {
    try {
      await historyClear();
      setEntries([]);
      setMessage('History cleared');
    } catch {
      setMessage('Clear failed');
    } finally {
      setClearOpen(false);
    }
  }

  return (
    <div className="settings" data-testid="history-screen">
      <label className="search">
        <input
          type="search"
          data-testid="history-search"
          placeholder="Search refinements…"
          aria-label="Search history"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <button
        className="btn btn--sm btn--danger"
        data-testid="history-clear"
        disabled={entries.length === 0}
        onClick={() => setClearOpen(true)}
      >
        Clear history
      </button>

      {entries.length === 0 ? (
        <section className="sec">
          <div className="grp">
            <div className="empty" data-testid="history-empty">
              <p style={{ fontSize: 16, color: 'var(--text)', margin: '0 0 6px' }}>No refinements yet</p>
              <p className="muted" style={{ margin: 0 }}>
                Select text in any app and press your capture hotkey — your redrafts will show up here.
              </p>
            </div>
          </div>
        </section>
      ) : filtered.length === 0 ? (
        <section className="sec">
          <div className="grp">
            <div className="empty" data-testid="history-no-results">
              <p style={{ fontSize: 16, color: 'var(--text)', margin: '0 0 6px' }}>No matches</p>
              <p className="muted" style={{ margin: 0 }}>
                Nothing in your history matches that search. Try a different term.
              </p>
            </div>
          </div>
        </section>
      ) : (
        <div className="grp list" data-testid="history-list">
          {filtered.map((entry) => (
            <article className="hrow" data-testid="history-row" key={entry.id}>
              <div className="hrow__main">
                <div className="hrow__meta">
                  <span className="mono tiny muted">{formatWhen(entry.createdAt)}</span>
                  <span className="chip mono">{entry.model}</span>
                  {entry.command && <span className="tag">{entry.command}</span>}
                </div>
                <div className="diff trunc">
                  <span className="diff__orig">{entry.original}</span>
                  <span className="diff__arrow"> {'→'} </span>
                  <span className="diff__refined">{entry.refined}</span>
                </div>
              </div>
              <div className="hrow__actions">
                <button
                  className="btn btn--sm"
                  data-testid="history-view"
                  aria-label="View full refinement"
                  onClick={() => setDetailEntry(entry)}
                >
                  View
                </button>
                <button
                  className="btn btn--sm"
                  data-testid="history-restore"
                  onClick={() => restore(entry)}
                >
                  Restore original
                </button>
                <button
                  className="btn btn--sm"
                  data-testid="history-rerefine"
                  onClick={() => reRefine(entry)}
                >
                  Re-refine
                </button>
                <button
                  className="btn btn--sm"
                  data-testid="history-copy"
                  aria-label="Copy refined text"
                  onClick={() => copy(entry)}
                >
                  Copy
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
      {message && (
        <div role="status" className="muted tiny" style={{ marginTop: 8 }}>
          {message}
        </div>
      )}

      {detailEntry && (
        <div className="modal-back" style={{ display: 'grid' }} role="dialog" aria-modal="true" data-testid="history-detail">
          <div className="modal">
            <h2 className="modal__title">Refinement detail</h2>
            <div className="hrow__meta" style={{ marginBottom: 14 }}>
              <span className="mono tiny muted">{formatWhen(detailEntry.createdAt)}</span>
              <span className="chip mono">{detailEntry.model}</span>
              {detailEntry.command && <span className="tag">{detailEntry.command}</span>}
            </div>
            <div className="diff-block" style={{ marginBottom: 14 }}>
              <span className="lbl before">Original</span>
              <div className="diff" data-testid="history-detail-original">
                <span className="diff__orig">{detailEntry.original}</span>
              </div>
            </div>
            <div className="diff-block">
              <span className="lbl after">Refined</span>
              <div className="diff" data-testid="history-detail-refined">
                <span className="diff__refined">{detailEntry.refined}</span>
              </div>
            </div>
            <div className="modal__foot">
              <button className="btn" data-testid="history-detail-close" onClick={() => setDetailEntry(null)}>
                Close
              </button>
              <button
                className="btn"
                data-testid="history-detail-restore"
                onClick={async () => {
                  await restore(detailEntry);
                  setDetailEntry(null);
                }}
              >
                Restore original
              </button>
              <button
                className="btn btn--primary"
                data-testid="history-detail-copy"
                onClick={() => copy(detailEntry)}
              >
                Copy refined
              </button>
            </div>
          </div>
        </div>
      )}

      {clearOpen && (
        <div className="modal-back" style={{ display: 'grid' }} role="alertdialog" aria-modal="true" data-testid="history-clear-modal">
          <div className="modal">
            <h2 className="modal__title">Clear history?</h2>
            <p className="muted" style={{ margin: 0 }}>
              Delete all <strong>{entries.length}</strong> {entries.length === 1 ? 'entry' : 'entries'}? This
              can&apos;t be undone — originals you haven&apos;t restored will be gone for good.
            </p>
            <div className="modal__foot">
              <button className="btn" data-testid="history-clear-cancel" onClick={() => setClearOpen(false)}>
                Cancel
              </button>
              <button
                className="btn btn--primary btn--danger"
                data-testid="history-clear-confirm"
                onClick={clearAll}
              >
                Delete all {entries.length}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
