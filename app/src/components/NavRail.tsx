'use client';

// The shared icon rail (wireframes/index.html's `.rail`, controls/index.json's
// `rail-*` + `theme-toggle` testids) that A14 renders around every settings
// screen. Navigation is in-app (a section switcher in App.tsx) rather than URL
// routing, so each rail button calls `onNavigate(sectionId)` instead of being
// an <a href>. The theme-toggle flips light/dark and persists it via theme.ts
// (settings_set 'theme'), matching the wireframe's rail toggle behavior.
//
// The Models/Presets/History sections don't have Phase A screens yet (Phase
// B/C); their rail buttons still render (the rail is the whole app's chrome)
// and navigate — App.tsx shows a "coming soon" placeholder for those until
// their screens land, so the rail never has dead buttons.

import type { ReactNode } from 'react';
import { loadTheme, setTheme, toggledTheme, type Theme } from '@/lib/theme';

/** The sections reachable from the rail. */
export type Section =
  | 'general'
  | 'connections'
  | 'models'
  | 'behavior'
  | 'presets'
  | 'history';

interface RailItem {
  id: Section;
  label: string;
  icon: ReactNode;
}

// Icons lifted verbatim from docs/wireframes/index.html's rail so the shell
// matches the wireframe 1:1.
export const RAIL_ITEMS: RailItem[] = [
  {
    id: 'general',
    label: 'General',
    icon: (
      <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6" />
    ),
  },
  {
    id: 'connections',
    label: 'Connections',
    icon: <path d="M9 7V3M15 7V3M7 7h10v4a5 5 0 0 1-10 0V7ZM12 15v6" />,
  },
  {
    id: 'models',
    label: 'Models',
    icon: (
      <>
        <rect x="6" y="6" width="12" height="12" rx="2" />
        <path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" />
      </>
    ),
  },
  {
    id: 'behavior',
    label: 'Behavior',
    icon: (
      <path d="m3 21 9-9M14 4l1.5 1.5M18 3l.7.7M20 8l-6 6M12 2l1 3 3 1-3 1-1 3-1-3-3-1 3-1 1-3Z" />
    ),
  },
  {
    id: 'presets',
    label: 'Presets',
    icon: (
      <>
        <rect x="3" y="4" width="18" height="16" rx="2" />
        <path d="M7 9l3 3-3 3M13 15h4" />
      </>
    ),
  },
  {
    id: 'history',
    label: 'History',
    icon: (
      <>
        <path d="M3 3v5h5" />
        <path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" />
        <path d="M12 7v5l4 2" />
      </>
    ),
  },
];

function RailIcon({ children }: { children: ReactNode }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
      {children}
    </svg>
  );
}

export interface NavRailProps {
  /** The section currently shown, so its rail button renders as active. */
  active: Section;
  /** Called with the target section id when a rail button is activated. */
  onNavigate: (section: Section) => void;
}

export default function NavRail({ active, onNavigate }: NavRailProps) {
  const onToggleTheme = async () => {
    const current: Theme = await loadTheme();
    await setTheme(toggledTheme(current));
  };

  return (
    <nav className="rail" aria-label="Primary" data-testid="icon-rail">
      <button
        className="rail__logo"
        aria-label="redrafter home"
        data-testid="rail-logo"
        onClick={() => onNavigate('general')}
      >
        R
      </button>

      {RAIL_ITEMS.map((item) => (
        <button
          key={item.id}
          className={`rail-btn${active === item.id ? ' active' : ''}`}
          aria-label={item.label}
          aria-current={active === item.id ? 'page' : undefined}
          title={item.label}
          data-testid={`rail-${item.id}`}
          onClick={() => onNavigate(item.id)}
        >
          <RailIcon>{item.icon}</RailIcon>
        </button>
      ))}

      <div className="rail__spacer" />

      <button
        className="rail-btn"
        id="theme-toggle"
        aria-label="Toggle theme"
        title="Toggle light / dark"
        data-testid="theme-toggle"
        onClick={onToggleTheme}
      >
        <RailIcon>
          <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z" />
        </RailIcon>
      </button>
    </nav>
  );
}
