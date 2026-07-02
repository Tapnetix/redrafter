// The main '/' entry: the real app shell (A14). A1b seeded this file with a
// placeholder; A14 replaces its body to render <App/>, which owns the boot
// flow (onboarding -> first-run -> settings) and the NavRail chrome. The
// standalone route pages under app/src/app/{onboarding,general,...} stay for
// the Phase-A per-screen E2E specs; this is the composed entry.
import App from '@/App';

export default function Home() {
  return <App />;
}
