'use client';

// The secondary settings sidebar (wireframes/index.html's `.sidebar`,
// controls/index.json's `nav-*`/`sidebar-active-model` testids) that A14
// renders alongside the icon rail. Like `NavRail`, navigation is in-app (the
// same section switcher in App.tsx) rather than URL routing, so each nav item
// calls `onNavigate(sectionId)` instead of being an <a href>.
//
// `sidebar-active-model` reads the same "first connection with an enabled
// model" Phase A treats as the whole app's active model (mirrors the
// backend's `active_provider` in `lib.rs`) — there's no Models screen yet to
// pick a different one, so this is a summary, not a switcher; clicking it
// still routes to the Models section (rendered as "coming soon" by App.tsx
// until Phase B/C builds it), matching `controls/index.json`'s declared
// behavior for that testid.

import { useEffect, useState } from 'react';
import { RAIL_ITEMS, RailIcon, type Section } from '@/components/NavRail';
import { connectionList } from '@/lib/ipc';

export interface SidebarProps {
  /** The section currently shown, so its nav item renders as active. */
  active: Section;
  /** Called with the target section id when a nav item is activated. */
  onNavigate: (section: Section) => void;
}

export default function Sidebar({ active, onNavigate }: SidebarProps) {
  const [activeModel, setActiveModel] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    connectionList()
      .then((connections) => {
        if (cancelled) return;
        const withModel = connections.find((c) => c.enabledModels.length > 0);
        setActiveModel(withModel ? withModel.enabledModels[0] : null);
      })
      .catch(() => {
        if (!cancelled) setActiveModel(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <aside className="sidebar" aria-label="Settings sections" data-testid="sidebar">
      <div className="sidebar__head">
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
          <span className={`status-dot ${activeModel ? 'green' : 'amber'}`} aria-hidden="true" />
          <span className="nav-item__label mono">{activeModel ?? 'No model selected'}</span>
        </button>
      </div>
    </aside>
  );
}
