// Standalone route so the Presets screen (app/src/screens/Presets.tsx) can
// be visited directly in isolation, matching the pattern in
// app/src/app/models/page.tsx.
import Presets from '@/screens/Presets';

export default function PresetsPage() {
  return <Presets />;
}
