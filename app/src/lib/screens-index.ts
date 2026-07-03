'use client';

// The single source of truth for the app's settings screens (C17). Before
// this, three places independently hard-coded the same screen list — the
// icon rail's `RAIL_ITEMS` (`NavRail.tsx`), the section-title map, and the
// section switcher (both in `App.tsx`) — so every screen a phase added
// (Models/B8, Presets/C3, History/C4) meant editing all three in lockstep.
// This registry collapses the section *list* (order, id, title, and how each
// screen mounts) into one array those consumers derive from, so adding or
// reordering a screen happens in exactly one place.
//
// This is a plain `.ts` module (no JSX): it holds the screen *components* and
// a `props` factory rather than rendered elements, so `App.tsx` does the
// actual mounting (`<Component {...props(navigate)} />`). The rail/sidebar
// icons — inherently JSX markup — stay in `NavRail.tsx`, keyed by these same
// ids, so the icon lookup and the section list still can't drift (a section
// here with no icon there renders a blank rail button, caught by NavRail's
// own "a button for every navigable section" test).
//
// The menu-bar Tray and the Capture panel are each their own OS window (wired
// natively in `lib.rs` / created from `tauri.conf.json`), not sections of
// this settings switcher, so they are deliberately not in this registry.

import type { ComponentType } from 'react';
import General from '@/screens/General';
import Behavior from '@/screens/Behavior';
import Connections from '@/screens/Connections';
import Models from '@/screens/Models';
import Presets from '@/screens/Presets';
import History from '@/screens/History';

/** The settings sections reachable from the rail/sidebar switcher. */
export type Section = 'general' | 'connections' | 'models' | 'behavior' | 'presets' | 'history';

/** One registered settings screen: its nav id/title, the component App.tsx
 * mounts for it, and (for the screens that cross-link to another section)
 * a factory that builds that component's props from the shared navigate
 * callback. */
export interface ScreenDef {
  /** The section key the switcher routes on and every `rail-*`/`nav-*`
   * testid is built from. */
  id: Section;
  /** Shown in both the nav item's label and the main topbar. */
  title: string;
  /** The screen component App.tsx renders for this section. */
  Component: ComponentType<Record<string, never>> | ComponentType<{ onNavigateToModels: () => void }> | ComponentType<{ onNavigateToConnections: () => void }>;
  /** Builds the component's props from the shared section-navigation
   * callback. Omitted for the screens that take none. */
  props?: (navigate: (section: Section) => void) => Record<string, unknown>;
}

export const SCREENS: ScreenDef[] = [
  { id: 'general', title: 'General', Component: General },
  {
    id: 'connections',
    title: 'Connections',
    Component: Connections,
    props: (navigate) => ({ onNavigateToModels: () => navigate('models') }),
  },
  {
    id: 'models',
    title: 'Models',
    Component: Models,
    props: (navigate) => ({ onNavigateToConnections: () => navigate('connections') }),
  },
  { id: 'behavior', title: 'Behavior', Component: Behavior },
  { id: 'presets', title: 'Presets', Component: Presets },
  { id: 'history', title: 'History', Component: History },
];

/** The section titles keyed by id — derived from {@link SCREENS} for the
 * topbar and any title lookup. */
export const SECTION_TITLES: Record<Section, string> = Object.fromEntries(
  SCREENS.map((screen) => [screen.id, screen.title]),
) as Record<Section, string>;

/** Looks up a screen's registry entry by id. */
export function screenById(id: Section): ScreenDef {
  const found = SCREENS.find((screen) => screen.id === id);
  if (!found) {
    throw new Error(`unknown screen id: ${id}`);
  }
  return found;
}
