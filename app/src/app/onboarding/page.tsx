'use client';

// Thin route so the Onboarding screen (and its S17 E2E spec) can be
// exercised in isolation before A14 wires up the real app shell/router
// (app/src/App.tsx + app/src/app/page.tsx). A14 supersedes this route once
// the full Onboarding -> FirstRun flow is composed there.

import { useState } from 'react';
import Onboarding from '@/screens/Onboarding';

export default function OnboardingPage() {
  const [continued, setContinued] = useState(false);

  if (continued) {
    return <div data-testid="onboarding-continued">Continuing to first-run setup…</div>;
  }

  return <Onboarding onContinue={() => setContinued(true)} />;
}
