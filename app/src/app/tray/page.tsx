// Standalone route so the Tray screen (app/src/screens/Tray.tsx) can be
// visited directly in isolation, matching the pattern in
// app/src/app/models/page.tsx.
import Tray from '@/screens/Tray';

export default function TrayPage() {
  return <Tray />;
}
