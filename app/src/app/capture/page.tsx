// Standalone route so the Capture screen (app/src/screens/Capture.tsx) can
// be visited directly in isolation before A14 wires the real hotkey-
// triggered overlay window and shared app composition. In the real app the
// global hotkey opens this panel in its own window; this route stands in
// for that trigger for E2E/manual testing.
import Capture from '@/screens/Capture';
import '../../../../docs/wireframes/app.css';

export default function CapturePage() {
  return <Capture />;
}
