'use client';

// The secondary settings sidebar (wireframes/index.html's `.sidebar`,
// controls/index.json's `nav-*`/`sidebar-active-model` testids) that A14
// renders alongside the icon rail. Like `NavRail`, navigation is in-app (the
// same section switcher in App.tsx) rather than URL routing, so each nav item
// calls `onNavigate(sectionId)` instead of being an <a href>.
//
// `sidebar-active-model` shows the whole app's active model. B23 wires it to
// the shared `model-store` (B8's `models_list` active model, refreshed when a
// pick is made anywhere): App.tsx passes that label in via `activeModelLabel`.
// When no label is supplied (e.g. Sidebar rendered in isolation), it falls
// back to its own read of the first connection with an enabled model (the
// Phase A `active_provider` heuristic), so the component still works
// standalone. Clicking it routes to the Models section (S26's switcher),
// matching `controls/index.json`'s declared behavior for that testid.

import { useEffect, useState } from 'react';
import { RAIL_ITEMS, RailIcon, type Section } from '@/components/NavRail';
import { connectionList } from '@/lib/ipc';
import { NO_MODEL_LABEL } from '@/lib/model-store';

export interface SidebarProps {
  /** The section currently shown, so its nav item renders as active. */
  active: Section;
  /** Called with the target section id when a nav item is activated. */
  onNavigate: (section: Section) => void;
  /**
   * The active-model label from the shared `model-store` (App.tsx supplies
   * it). When omitted, the Sidebar derives its own from `connection_list`
   * so it still renders a sensible summary in isolation.
   */
  activeModelLabel?: string;
}

export default function Sidebar({ active, onNavigate, activeModelLabel }: SidebarProps) {
  const [fallbackModel, setFallbackModel] = useState<string | null>(null);

  useEffect(() => {
    // Only needed for the standalone fallback path — when App.tsx supplies
    // `activeModelLabel` from the shared store, skip the extra fetch.
    if (activeModelLabel !== undefined) return;
    let cancelled = false;
    connectionList()
      .then((connections) => {
        if (cancelled) return;
        const withModel = connections.find((c) => c.enabledModels.length > 0);
        setFallbackModel(withModel ? withModel.enabledModels[0] : null);
      })
      .catch(() => {
        if (!cancelled) setFallbackModel(null);
      });
    return () => {
      cancelled = true;
    };
  }, [activeModelLabel]);

  const label = activeModelLabel ?? fallbackModel ?? NO_MODEL_LABEL;
  const hasModel = label !== NO_MODEL_LABEL;

  return (
    <aside className="sidebar" aria-label="Settings sections" data-testid="sidebar">
      <div className="sidebar__head" style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
        <img src="/logo.png" alt="" width={22} height={22} draggable={false} style={{ flex: 'none' }} />
        <div className="app-name">
          redraft<span>er</span>
        </div>
      </div>
      <div className="sidebar__scroll">
        <div className="nav-group" data-testid="group-settings">
          <div className="nav-group__label">Settings</div>
          {RAIL_ITEMS.map((item) => (
            <button
              key={item.id}
              className={`nav-item${active === item.id ? ' active' : ''}`}
              data-testid={`nav-${item.id}`}
              onClick={() => onNavigate(item.id)}
            >
              <span className="nav-item__icon">
                <RailIcon>{item.icon}</RailIcon>
              </span>
              <span className="nav-item__label">{item.label}</span>
            </button>
          ))}
        </div>
      </div>
      <div className="sidebar__foot">
        <button
          className="nav-item"
          data-testid="sidebar-active-model"
          style={{ border: '1px solid var(--border)' }}
          onClick={() => onNavigate('models')}
        >
          <span className={`status-dot ${hasModel ? 'green' : 'amber'}`} aria-hidden="true" />
          <span className="nav-item__label mono">{label}</span>
        </button>
      </div>
    </aside>
  );
}
