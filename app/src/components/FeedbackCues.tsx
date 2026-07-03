'use client';

// Global feedback-cue surface (SC13): renders the in-flight "Refining…"
// spinner and the completion HUD flash driven by the `useFeedbackCues` hook,
// which listens to the backend's `refine-feedback-start`/`-done` events. Kept
// as a tiny, always-mounted overlay in the main app window so an in-flight
// refine is reflected even when the Capture panel isn't the focused surface
// (the Capture panel keeps its own inline Refining state; this is the global
// mirror).
//
// See `feedback-cues.ts` for the documented follow-up: the HUD here is a brief
// in-app flash, not yet the caret-anchored always-on-top overlay window.

import { useFeedbackCues } from '@/lib/feedback-cues';

export default function FeedbackCues() {
  const { spinner, hud } = useFeedbackCues();

  return (
    <div className="feedback-cues" aria-live="polite">
      {spinner && (
        <div data-testid="feedback-spinner" role="status" className="feedback-cue feedback-cue--spinner">
          <span className="feedback-cue__spinner-dot" aria-hidden="true" />
          Refining…
        </div>
      )}
      {hud && (
        <div data-testid="feedback-hud" role="status" className="feedback-cue feedback-cue--hud">
          Refined
        </div>
      )}
    </div>
  );
}
