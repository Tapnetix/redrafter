'use client';

// The in-flight HUD chip (`hud.rs`): a tiny always-on-top window shown beside
// the pointer while a refine runs, so a slow refine is visible without the
// settings window being open.
//
// Deliberately static. Its visibility and position are owned by Rust
// (`hud::show`/`hud::hide`, driven by the same tray status transitions as the
// menu-bar spinner) rather than by a frontend event listener — one source of
// truth, and nothing to go stale if an event is missed while the webview is
// still warming up.
//
// The background is painted opaque and edge-to-edge on purpose: the window is
// not transparent (that needs macOS private APIs), so any rounding here would
// just expose the window's own backdrop at the corners.

export default function HudPage() {
  return (
    <div
      data-testid="hud-chip"
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
        background: 'var(--surface-raised, #1f2024)',
        color: 'var(--text, #f5f6f8)',
        font: '500 13px/1 var(--font-body, system-ui, -apple-system, sans-serif)',
        userSelect: 'none',
        cursor: 'default',
        overflow: 'hidden',
      }}
    >
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
      <style>{`
        @keyframes hud-spin { to { transform: rotate(360deg); } }
        @media (prefers-reduced-motion: reduce) {
          [data-testid="hud-chip"] > span { animation-duration: 2.4s; }
        }
      `}</style>
    </div>
  );
}
