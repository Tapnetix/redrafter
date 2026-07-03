'use client';

// History settings screen (wireframes/history.html, controls/history.json).
// C4 (this file) is the sole owner of the past-refinements list plus its
// core actions: restoring a past entry's original text back into the
// focused app, and re-refining a past entry's original to produce a fresh
// result. The backend this calls through `@/lib/ipc`
// (`app/src-tauri/src/history.rs`) is also C4's; C17 registers those
// commands in the Tauri invoke handler and wires `history_append` into
// every successful `refine` — until then, calling them against a real
// backend rejects (mirrors `Models.tsx`'s note on B23).
//
// Search/detail-view/copy/clear-all (`history-search`/`history-view`/
// `history-copy`/`history-clear`/the detail dialog) are later tasks'
// (C12-C15) job to wire up; this screen doesn't render placeholders for
// them yet to avoid speculative markup nothing exercises.
import { useEffect, useState } from 'react';
import {
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

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [message, setMessage] = useState('');

  const load = () => {
    historyList()
      .then(setEntries)
      .catch(() => setEntries([]));
  };

  useEffect(() => {
    load();
  }, []);

  async function restore(entry: HistoryEntry) {
    await historyRestore(entry.id);
    setMessage('Original restored');
  }

  async function reRefine(entry: HistoryEntry) {
    const updated = await historyReRefine({ id: entry.id });
    setEntries((prev) => [updated, ...prev]);
    setMessage('Re-refined');
  }

  return (
    <div className="settings" data-testid="history-screen">
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
      ) : (
        <div className="grp list" data-testid="history-list">
          {entries.map((entry) => (
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
    </div>
  );
}
