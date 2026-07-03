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
// Behavior, and Tray as the screens wired into the section switcher here.
// alongside Phase A's General/Behavior. The Capture panel and the menu-bar
// Tray are each normally their own window (wired natively in lib.rs /
// created from tauri.conf.json's trayIcon); their standalone `/capture` and
// `/tray` routes stand in for those surfaces here.
//
// B23 wires the shared `model-store` in: the Sidebar's active-model
// indicator now reflects B8's real active model (refreshed whenever the
// section changes, so a pick made on the Models screen shows up on return),
// rather than only the Phase A first-enabled-connection heuristic.

import { useCallback, useEffect, useState } from 'react';
import NavRail, { type Section } from '@/components/NavRail';
import Sidebar from '@/components/Sidebar';
import Onboarding from '@/screens/Onboarding';
import FirstRun from '@/screens/FirstRun';
import General from '@/screens/General';
import Behavior from '@/screens/Behavior';
import Connections from '@/screens/Connections';
import Models from '@/screens/Models';
import History from '@/screens/History';
import Presets from '@/screens/Presets';
import { getPermissionStatus, connectionList } from '@/lib/ipc';
import { useModelStore } from '@/lib/model-store';
import { applyTheme, loadTheme } from '@/lib/theme';

/** Where the boot check has decided the user should land. */
type Route = 'loading' | 'onboarding' | 'first-run' | 'settings';

const SECTION_TITLES: Record<Section, string> = {
  general: 'General',
  connections: 'Connections',
  models: 'Models',
  behavior: 'Behavior',
  presets: 'Presets',
  history: 'History',
};

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

function ComingSoon({ section }: { section: Section }) {
  return (
    <div className="settings" data-testid={`section-${section}-placeholder`}>
      <section className="sec">
        <h2 className="sec__title">{SECTION_TITLES[section]}</h2>
        <p className="muted tiny" style={{ margin: 0 }}>
          The {SECTION_TITLES[section]} screen arrives in a later phase.
        </p>
      </section>
    </div>
  );
}

function SectionView({ section, onNavigate }: { section: Section; onNavigate: (section: Section) => void }) {
  switch (section) {
    case 'general':
      return <General />;
    case 'behavior':
      return <Behavior />;
    case 'connections':
      return <Connections onNavigateToModels={() => onNavigate('models')} />;
    case 'models':
      return <Models onNavigateToConnections={() => onNavigate('connections')} />;
    case 'history':
      return <History />;
    case 'presets':
      return <Presets />;
    default:
      return <ComingSoon section={section} />;
  }
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
