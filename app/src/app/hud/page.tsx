'use client';

// The in-flight / failure chip (`hud.rs`): a tiny always-on-top window shown
// beside the pointer while a refine runs, and again — in red — when one fails.
//
// The failure half matters more than it looks. A refine triggered by the global
// hotkey hands its `Result` to a JoinHandle that the shortcut handler drops, so
// an error was never read, logged or surfaced: the spinner appeared, vanished,
// and that was the entire user-visible account of a failed refine. This is
// where it gets said.
//
// Visibility and position are owned by Rust (`hud::show_refining` /
// `hud::show_error`); this page only renders whatever state it is told about,
// read once on mount and then via the `hud:state` event.

import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { hudState, type HudState } from '@/lib/ipc';

export default function HudPage() {
  const [state, setState] = useState<HudState | null>(null);

  const load = useCallback(async () => {
    try {
      setState((await hudState()) ?? null);
    } catch {
      // Nothing useful to say if even this fails; the window stays as-is.
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      const p = listen<HudState>('hud:state', (event) => setState(event.payload ?? null));
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
  }, []);

  const failed = state?.kind === 'error';

  return (
    <div
      data-testid="hud-chip"
      data-kind={state?.kind ?? 'refining'}
      role="status"
      aria-live="polite"
      style={{
        position: 'fixed',
        inset: 0,
        margin: 0,
        display: 'flex',
        alignItems: 'center',
        gap: 9,
        padding: '0 14px',
        background: failed ? 'var(--danger, #d0433b)' : 'var(--surface-raised, #1f2024)',
        color: failed ? '#fff' : 'var(--text, #f5f6f8)',
        font: '500 13px/1.35 var(--font-body, system-ui, -apple-system, sans-serif)',
        userSelect: 'none',
        cursor: 'default',
        overflow: 'hidden',
      }}
    >
      {failed ? (
        <>
          <span aria-hidden="true" style={{ flex: 'none', fontSize: 14 }}>
            ⚠
          </span>
          <span
            data-testid="hud-error"
            title={state?.text}
            style={{
              // The window is small and a provider error can be long; keep it
              // to two lines rather than clipping mid-word.
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              overflow: 'hidden',
            }}
          >
            {state?.text || 'Refine failed'}
          </span>
        </>
      ) : (
        <>
          <span
            aria-hidden="true"
            style={{
              width: 13,
              height: 13,
              flex: 'none',
              borderRadius: '50%',
              border: '2px solid rgba(255,255,255,0.22)',
              borderTopColor: 'var(--accent-500, #7b80e0)',
              animation: 'hud-spin 700ms linear infinite',
            }}
          />
          Refining…
        </>
      )}
      <style>{`
        @keyframes hud-spin { to { transform: rotate(360deg); } }
        @media (prefers-reduced-motion: reduce) {
          [data-testid="hud-chip"] > span { animation-duration: 2.4s; }
        }
      `}</style>
    </div>
  );
}
