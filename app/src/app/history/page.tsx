// Standalone route so the History screen (app/src/screens/History.tsx) can
// be visited directly in isolation, matching the pattern in
// app/src/app/models/page.tsx.
import History from '@/screens/History';

export default function HistoryPage() {
  return <History />;
}
