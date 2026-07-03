// Frontend consumer for the refine feedback-cue events the backend emits
// around every refine (`run_refine` in `app/src-tauri/src/lib.rs`):
//
//   - `refine-feedback-start` — fired right before the pipeline runs, with a
//     payload of the enabled in-flight cues (`["spinner"]`, `["spinner",
//     "hud"]`, …). Drives the global in-app "Refining…" spinner.
//   - `refine-feedback-done`  — fired right after the pipeline finishes (on
//     success OR failure), with the enabled done cues (spinner/HUD to clear,
//     plus `"sound"` when the completion chime is on). Clears the spinner and,
//     when the HUD cue is present, flashes a brief completion HUD.
//
// Before this, both events were emitted into the void (nothing on the frontend
// listened) — this hook is what actually renders SC13's spinner/HUD. The
// spinner is a *global* app-window indicator (not just the Capture panel's own
// Refining state) so an in-flight refine is visible even when Capture isn't
// the focused surface.
//
// FOLLOW-UP (documented, intentionally out of scope here): a true near-cursor
// HUD is a borderless, always-on-top overlay *window* positioned at the caret
// — that needs a separate Tauri window (label + capability + creation glue),
// which is a larger, platform-sensitive change than this fix. The HUD below is
// a brief in-app completion flash rendered inside the app window instead; the
// caret-anchored overlay window is a follow-up. The cue is no longer dropped
// on the floor either way.
//
// The completion SOUND is played natively by the backend (`feedback.rs`'s
// `play_completion_sound`); the `"sound"` cue reaching the frontend is purely
// informational — this hook never plays audio itself, so the two never
// double-fire.

import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** Tauri event name emitted right before a refine runs (matches
 * `REFINE_FEEDBACK_START_EVENT` in `lib.rs`). */
export const REFINE_FEEDBACK_START_EVENT = 'refine-feedback-start';
/** Tauri event name emitted right after a refine finishes (matches
 * `REFINE_FEEDBACK_DONE_EVENT` in `lib.rs`). */
export const REFINE_FEEDBACK_DONE_EVENT = 'refine-feedback-done';

/** One feedback cue, matching the backend `FeedbackCue` enum's snake_case
 * serialization. */
export type FeedbackCue = 'spinner' | 'hud' | 'sound';

/** How long the completion HUD flash stays up (ms) before auto-hiding. */
export const HUD_FLASH_MS = 1400;

/** The rendered feedback-cue state a consumer (`FeedbackCues`) reflects. */
export interface FeedbackCueState {
  /** The global "Refining…" spinner is visible (refine in flight). */
  spinner: boolean;
  /** The completion HUD flash is currently showing. */
  hud: boolean;
}

/**
 * Subscribes to the refine feedback-cue events for the lifetime of the
 * component and returns the derived spinner/HUD state.
 *
 * - Spinner: shown on `-start` when the payload enables it, hidden on `-done`
 *   (always — a spinner must never get stuck showing if the refine failed).
 * - HUD: on `-done`, when the payload enables it, a brief completion flash is
 *   shown and auto-hidden after {@link HUD_FLASH_MS}.
 */
export function useFeedbackCues(): FeedbackCueState {
  const [spinner, setSpinner] = useState(false);
  const [hud, setHud] = useState(false);
  const hudTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlistenStart: UnlistenFn | undefined;
    let unlistenDone: UnlistenFn | undefined;

    const flashHud = () => {
      setHud(true);
      if (hudTimer.current) clearTimeout(hudTimer.current);
      hudTimer.current = setTimeout(() => setHud(false), HUD_FLASH_MS);
    };

    void listen<FeedbackCue[]>(REFINE_FEEDBACK_START_EVENT, (event) => {
      const cues = event.payload ?? [];
      if (cues.includes('spinner')) setSpinner(true);
    }).then((un) => {
      if (disposed) un();
      else unlistenStart = un;
    });

    void listen<FeedbackCue[]>(REFINE_FEEDBACK_DONE_EVENT, (event) => {
      const cues = event.payload ?? [];
      // The refine finished (success or failure): always clear the spinner.
      setSpinner(false);
      if (cues.includes('hud')) flashHud();
      // 'sound' is played natively by the backend — nothing to do here.
    }).then((un) => {
      if (disposed) un();
      else unlistenDone = un;
    });

    return () => {
      disposed = true;
      unlistenStart?.();
      unlistenDone?.();
      if (hudTimer.current) clearTimeout(hudTimer.current);
    };
  }, []);

  return { spinner, hud };
}
