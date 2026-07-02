// Standalone route so the General screen (app/src/screens/General.tsx) can
// be visited directly in isolation before A14 wires the real app router
// (App.tsx) and shared NavRail chrome around every settings screen.
import General from '@/screens/General';
import '../../../../docs/wireframes/app.css';

export default function GeneralPage() {
  return <General />;
}
