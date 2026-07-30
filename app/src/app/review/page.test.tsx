import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import ReviewPage from './page';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  reviewPending: vi.fn(),
  reviewAccept: vi.fn(),
  reviewDiscard: vi.fn(),
}));

// The panel refreshes on a `review:pending` event so a second refine replaces
// the draft instead of leaving a stale one to be accepted by mistake.
const { handlers } = vi.hoisted(() => ({ handlers: {} as Record<string, () => void> }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: () => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => delete handlers[name]);
  }),
}));

const mockedIpc = vi.mocked(ipc);

const outcome = {
  original: 'i beleive this dont work',
  refined: "I believe this doesn't work.",
  model: 'claude-opus-5',
  status: 'pending_review' as const,
};

describe('Review panel', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.reviewPending.mockResolvedValue(outcome);
    mockedIpc.reviewAccept.mockResolvedValue(undefined);
    mockedIpc.reviewDiscard.mockResolvedValue(undefined);
  });

  it('shows the original, the refined draft and the model used', async () => {
    render(<ReviewPage />);

    expect(await screen.findByTestId('review-original')).toHaveTextContent('i beleive this dont work');
    expect(screen.getByTestId('review-refined')).toHaveTextContent("I believe this doesn't work.");
    expect(screen.getByTestId('review-model')).toHaveTextContent('claude-opus-5');
  });

  it('inserts the draft through review_accept, not inject_text directly', async () => {
    // review_accept is what hides the panel and restores focus first; calling
    // inject_text from here would paste into this window.
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    fireEvent.click(screen.getByTestId('review-accept'));

    await waitFor(() => expect(mockedIpc.reviewAccept).toHaveBeenCalledWith("I believe this doesn't work."));
  });

  it('inserts the edited text when the draft was changed', async () => {
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    fireEvent.click(screen.getByTestId('review-edit'));
    fireEvent.change(await screen.findByTestId('review-edit-field'), {
      target: { value: 'My own wording.' },
    });
    fireEvent.click(screen.getByTestId('review-accept'));

    await waitFor(() => expect(mockedIpc.reviewAccept).toHaveBeenCalledWith('My own wording.'));
  });

  it('discards without injecting anything', async () => {
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    fireEvent.click(screen.getByTestId('review-discard'));

    await waitFor(() => expect(mockedIpc.reviewDiscard).toHaveBeenCalledTimes(1));
    expect(mockedIpc.reviewAccept).not.toHaveBeenCalled();
  });

  it('accepts on Cmd+Enter and discards on Escape', async () => {
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    fireEvent.keyDown(window, { key: 'Enter', metaKey: true });
    await waitFor(() => expect(mockedIpc.reviewAccept).toHaveBeenCalledTimes(1));

    fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(mockedIpc.reviewDiscard).toHaveBeenCalledTimes(1));
  });

  it('keeps the draft on screen and explains when inserting fails', async () => {
    // Injection can genuinely fail (permission revoked, nothing focused).
    // Losing the draft at that point would lose the user's text.
    mockedIpc.reviewAccept.mockRejectedValue(new Error('permission_denied'));
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    fireEvent.click(screen.getByTestId('review-accept'));

    expect(await screen.findByTestId('review-error')).toHaveTextContent('permission_denied');
    expect(screen.getByTestId('review-refined')).toBeInTheDocument();
  });

  it('replaces the draft when a second refine pends', async () => {
    render(<ReviewPage />);
    await screen.findByTestId('review-refined');

    mockedIpc.reviewPending.mockResolvedValue({ ...outcome, refined: 'A newer draft.' });
    handlers['review:pending']();

    await waitFor(() =>
      expect(screen.getByTestId('review-refined')).toHaveTextContent('A newer draft.'),
    );
  });

  it('shows an empty state rather than throwing when nothing is pending', async () => {
    mockedIpc.reviewPending.mockResolvedValue(null);
    render(<ReviewPage />);

    expect(await screen.findByTestId('review-empty')).toBeInTheDocument();
    expect(screen.getByTestId('review-accept')).toBeDisabled();
  });
});
