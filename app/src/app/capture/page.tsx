'use client';

// Standalone route so the Capture screen (app/src/screens/Capture.tsx) can
// be visited directly in isolation before A14 wires the real hotkey-
// triggered overlay window and shared app composition. In the real app the
// global hotkey opens this panel in its own window; this route stands in
// for that trigger for E2E/manual testing.
//
// B11 additively wires an optional `?text=` query param to Capture's
// existing `capturedText` prop seam (see Capture.tsx's file header on why
// that seam exists) so E2E specs can drive the live command-parse preview
// over a specific selection containing `/rd`/`/m`/`/q`/`/lang` commands,
// without touching Capture.tsx or command-preview.ts (owned by B6/B4).
// Omitting the param preserves the prior behavior (Capture's own
// placeholder default).
import { Suspense } from 'react';
import { useSearchParams } from 'next/navigation';
import Capture from '@/screens/Capture';

function CapturePageInner() {
  const searchParams = useSearchParams();
  const text = searchParams.get('text');
  return <Capture capturedText={text ?? undefined} />;
}

export default function CapturePage() {
  return (
    <Suspense>
      <CapturePageInner />
    </Suspense>
  );
}
