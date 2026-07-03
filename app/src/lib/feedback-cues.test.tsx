import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import FeedbackCues from '@/components/FeedbackCues';
import {
  HUD_FLASH_MS,
  REFINE_FEEDBACK_DONE_EVENT,
  REFINE_FEEDBACK_START_EVENT,
  type FeedbackCue,
} from './feedback-cues';

// Capture the event handlers `useFeedbackCues` registers so the test can drive
// the two feedback-cue events itself, standing in for the backend's emits.
const handlers: Record<string, (event: { payload: FeedbackCue[] }) => void> = {};
const unlisten = vi.fn();

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, handler: (event: { payload: FeedbackCue[] }) => void) => {
    handlers[event] = handler;
    return Promise.resolve(unlisten);
  }),
}));

/** Fires a feedback-cue event into the registered handler, wrapped in `act` so
 * React flushes the resulting state update. */
function emit(event: string, payload: FeedbackCue[]) {
  act(() => {
    handlers[event]?.({ payload });
  });
}

describe('FeedbackCues (refine feedback-cue consumer)', () => {
  beforeEach(async () => {
    for (const key of Object.keys(handlers)) delete handlers[key];
    unlisten.mockClear();
    render(<FeedbackCues />);
    // Let the `listen(...)` promises resolve so the handlers are registered.
    await act(async () => {});
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('subscribes to both feedback-cue events on mount', () => {
    expect(listen).toHaveBeenCalledWith(REFINE_FEEDBACK_START_EVENT, expect.any(Function));
    expect(listen).toHaveBeenCalledWith(REFINE_FEEDBACK_DONE_EVENT, expect.any(Function));
  });

  it('shows the spinner on start and hides it on done', () => {
    expect(screen.queryByTestId('feedback-spinner')).not.toBeInTheDocument();

    emit(REFINE_FEEDBACK_START_EVENT, ['spinner']);
    expect(screen.getByTestId('feedback-spinner')).toBeInTheDocument();

    // The done event clears the spinner even though its payload still lists the
    // spinner cue (and 'sound', which the frontend never renders).
    emit(REFINE_FEEDBACK_DONE_EVENT, ['spinner', 'sound']);
    expect(screen.queryByTestId('feedback-spinner')).not.toBeInTheDocument();
  });

  it('does not show the spinner on start when the spinner cue is disabled', () => {
    // HUD-only config: start carries no spinner cue, so no global spinner.
    emit(REFINE_FEEDBACK_START_EVENT, ['hud']);
    expect(screen.queryByTestId('feedback-spinner')).not.toBeInTheDocument();
  });

  it('flashes the completion HUD on done only when the HUD cue is enabled', () => {
    vi.useFakeTimers();

    // Done without the HUD cue (spinner+sound config): no HUD flash.
    emit(REFINE_FEEDBACK_DONE_EVENT, ['spinner', 'sound']);
    expect(screen.queryByTestId('feedback-hud')).not.toBeInTheDocument();

    // Done with the HUD cue enabled: the completion flash shows...
    emit(REFINE_FEEDBACK_DONE_EVENT, ['hud']);
    expect(screen.getByTestId('feedback-hud')).toBeInTheDocument();

    // ...and auto-hides after the flash window.
    act(() => {
      vi.advanceTimersByTime(HUD_FLASH_MS);
    });
    expect(screen.queryByTestId('feedback-hud')).not.toBeInTheDocument();
  });

  it('never renders a "sound" element — the completion sound is played natively by the backend', () => {
    emit(REFINE_FEEDBACK_DONE_EVENT, ['sound']);
    expect(screen.queryByTestId('feedback-spinner')).not.toBeInTheDocument();
    expect(screen.queryByTestId('feedback-hud')).not.toBeInTheDocument();
  });
});
