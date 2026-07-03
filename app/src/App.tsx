'use client';

// The real app shell / composition root on the frontend side (A14). It owns
// the boot flow and the in-app section switcher that the standalone route
// pages (app/src/app/*/page.tsx) previously stood in for:
//
//   1. On boot, read permission + connections + theme from the backend.
//   2. An ungranted Accessibility permission routes to Onboarding (S17);
//      once granted with no connected provider, to FirstRun; otherwise into
//      the settings shell (NavRail + Sidebar chrome around the selected
//      screen).
//
// History (C4) and Presets (C3) join Models (B8), Connections (B7), General,
// and Behavior as the screens wired into the section switcher here. As of
// C17 that switcher (and the section-title map, and the nav rail/sidebar
// item list) all derive from the one shared `screens-index` registry rather
// than each re-listing the sections — so a new screen is registered in one
// place, not four. The Capture panel and the menu-bar Tray are each normally
// their own window (wired natively in lib.rs / created from
// tauri.conf.json's trayIcon); their standalone `/capture` and `/tray`
// routes stand in for those surfaces here.
//
// B23 wires the shared `model-store` in: the Sidebar's active-model
// indicator now reflects B8's real active model (refreshed whenever the
// section changes, so a pick made on the Models screen shows up on return),
// rather than only the Phase A first-enabled-connection heuristic.

import { useCallback, useEffect, useState, type ComponentType } from 'react';
import NavRail from '@/components/NavRail';
import Sidebar from '@/components/Sidebar';
import FeedbackCues from '@/components/FeedbackCues';
import Onboarding from '@/screens/Onboarding';
import FirstRun from '@/screens/FirstRun';
import { getPermissionStatus, connectionList } from '@/lib/ipc';
import { useModelStore } from '@/lib/model-store';
import { applyTheme, loadTheme } from '@/lib/theme';
import { SECTION_TITLES, screenById, type Section } from '@/lib/screens-index';

/** Where the boot check has decided the user should land. */
type Route = 'loading' | 'onboarding' | 'first-run' | 'settings';

/**
 * Decides the boot route from backend state: no Accessibility -> onboarding;
 * granted but no connected provider -> first-run; otherwise the settings shell.
 */
async function decideRoute(): Promise<Route> {
  try {
    const status = await getPermissionStatus();
    if (!status.granted) return 'onboarding';
  } catch {
    // If we can't even read permission state, start at onboarding — the safest
    // place, since nothing downstream works without Accessibility.
    return 'onboarding';
  }

  try {
    const connections = await connectionList();
    if (connections.length === 0) return 'first-run';
  } catch {
    // A failed connection read shouldn't strand the user; treat it as "no
    // provider yet" and send them to first-run to add one.
    return 'first-run';
  }

  return 'settings';
}

/** Mounts the registered screen for `section` from the shared
 * `screens-index`, threading the app's `navigate` callback into the screens
 * (Connections/Models) that cross-link to another section. Deriving this
 * from the registry (rather than a per-section `switch`) is what keeps a new
 * screen a one-line registry edit instead of another `App.tsx` change. */
function SectionView({ section, onNavigate }: { section: Section; onNavigate: (section: Section) => void }) {
  const { Component, props } = screenById(section);
  const resolvedProps = props ? props(onNavigate) : {};
  // The registry's `Component`/`props` pairing is validated at the type
  // level in `screens-index`; here they're combined dynamically, so a cast
  // to a permissive element type is unavoidable at the call site.
  const Screen = Component as ComponentType<Record<string, unknown>>;
  return <Screen {...resolvedProps} />;
}

export default function App() {
  const [route, setRoute] = useState<Route>('loading');
  const [section, setSection] = useState<Section>('general');
  const modelStore = useModelStore();

  // Boot: rehydrate the theme and decide where to land.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      applyTheme(await loadTheme());
      const next = await decideRoute();
      if (!cancelled) setRoute(next);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Re-run the boot decision after onboarding/first-run finish, so granting a
  // permission or adding a provider advances the user without a restart.
  const advance = useCallback(async () => {
    setRoute(await decideRoute());
  }, []);

  // Navigating between sections re-pulls the shared model store, so a model
  // picked on the Models screen is reflected in the Sidebar's indicator when
  // the user moves back to another section (the store is the single source
  // the tray + models + capture all read).
  const navigate = useCallback(
    (next: Section) => {
      setSection(next);
      void modelStore.refresh();
    },
    [modelStore],
  );

  if (route === 'loading') {
    return <div data-testid="app-loading" aria-busy="true" />;
  }

  if (route === 'onboarding') {
    return <Onboarding onContinue={advance} />;
  }

  if (route === 'first-run') {
    return <FirstRun onContinue={advance} />;
  }

  return (
    <div className="app" data-testid="app-shell">
      {/* Global in-flight spinner / completion HUD, driven by the backend's
          refine feedback-cue events (SC13) — mounted here so it reflects a
          refine even when the Capture panel isn't the focused surface. */}
      <FeedbackCues />
      <NavRail active={section} onNavigate={navigate} />
      <Sidebar active={section} onNavigate={navigate} activeModelLabel={modelStore.activeModelLabel} />
      <main className="main">
        <header className="topbar">
          <h1 className="topbar__title">{SECTION_TITLES[section]}</h1>
        </header>
        <div className="content" style={{ padding: 0 }}>
          <SectionView section={section} onNavigate={navigate} />
        </div>
      </main>
    </div>
  );
}
