'use client';

// A dedicated static route for the first-run screen so it is directly
// navigable (both for `app/e2e/specs/a7.spec.ts` and for a real first boot).
// A14 owns the app's real routing/redirect logic (`App.tsx`, root
// `page.tsx`): once permission is granted and no provider is connected, it
// sends the user here; `onContinue` below is a placeholder for "open the
// main window" until that composition root exists.
import { useRouter } from 'next/navigation';
import FirstRun from '@/screens/FirstRun';

export default function FirstRunPage() {
  const router = useRouter();
  return <FirstRun onContinue={() => router.push('/')} />;
}
