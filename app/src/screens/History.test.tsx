import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import History from './History';
import * as ipc from '@/lib/ipc';
import type { HistoryEntry } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  historyList: vi.fn(),
  historyGet: vi.fn(),
  historyRestore: vi.fn(),
  historyReRefine: vi.fn(),
  historyCopy: vi.fn(),
  historyClear: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

function entry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id: '1',
    original: 'we good with the release plan i think',
    refined: "We're good with the release plan.",
    model: 'claude-opus-4-6',
    createdAt: 1_700_000_000_000,
    ...overrides,
  };
}

describe('History', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('shows the empty state with no recorded refines', async () => {
    mockedIpc.historyList.mockResolvedValue([]);

    render(<History />);

    await screen.findByTestId('history-empty');
    expect(screen.queryByTestId('history-list')).not.toBeInTheDocument();
  });

  it('lists past refines with their original/refined text and model', async () => {
    mockedIpc.historyList.mockResolvedValue([
      entry({ id: '1', original: 'first original', refined: 'first refined' }),
      entry({ id: '2', original: 'second original', refined: 'second refined' }),
    ]);

    render(<History />);

    const rows = await screen.findAllByTestId('history-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('first original');
    expect(rows[0]).toHaveTextContent('first refined');
    expect(rows[0]).toHaveTextContent('claude-opus-4-6');
  });

  it('clicking Restore original calls history_restore for that entry', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);
    mockedIpc.historyRestore.mockResolvedValue('we good with the release plan i think');

    render(<History />);
    await screen.findByTestId('history-row');

    fireEvent.click(screen.getByTestId('history-restore'));

    await waitFor(() => expect(mockedIpc.historyRestore).toHaveBeenCalledWith('1'));
  });

  it('clicking Re-refine calls history_rerefine and prepends the new entry to the list', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);
    mockedIpc.historyReRefine.mockResolvedValue(
      entry({ id: '2', refined: 'a brand new refined result' }),
    );

    render(<History />);
    await screen.findByTestId('history-row');

    fireEvent.click(screen.getByTestId('history-rerefine'));

    await waitFor(() => expect(mockedIpc.historyReRefine).toHaveBeenCalledWith({ id: '1' }));
    const rows = await screen.findAllByTestId('history-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('a brand new refined result');
  });

  // ── Search (C14/S32) ──

  it('filters the list to rows matching the search text', async () => {
    mockedIpc.historyList.mockResolvedValue([
      entry({ id: '1', original: 'we good with the release plan', refined: 'We are good with the release plan.' }),
      entry({ id: '2', original: 'thanks for the update', refined: 'Thanks for the update!' }),
    ]);

    render(<History />);
    await screen.findAllByTestId('history-row');

    fireEvent.change(screen.getByTestId('history-search'), { target: { value: 'release plan' } });

    const rows = await screen.findAllByTestId('history-row');
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent('release plan');
  });

  it('shows a no-results state when nothing matches the search text', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1', original: 'we good with the release plan' })]);

    render(<History />);
    await screen.findAllByTestId('history-row');

    fireEvent.change(screen.getByTestId('history-search'), { target: { value: 'nothing matches this' } });

    await screen.findByTestId('history-no-results');
    expect(screen.queryByTestId('history-row')).not.toBeInTheDocument();
  });

  // ── Detail view (C13/S31) ──

  it('clicking View opens the detail dialog with the full original/refined/model/time', async () => {
    mockedIpc.historyList.mockResolvedValue([
      entry({
        id: '1',
        original: 'the full original selection text',
        refined: 'The full refined result text.',
        model: 'claude-opus-4-6',
      }),
    ]);

    render(<History />);
    await screen.findByTestId('history-row');

    fireEvent.click(screen.getByTestId('history-view'));

    const detail = await screen.findByTestId('history-detail');
    expect(screen.getByTestId('history-detail-original')).toHaveTextContent('the full original selection text');
    expect(screen.getByTestId('history-detail-refined')).toHaveTextContent('The full refined result text.');
    expect(detail).toHaveTextContent('claude-opus-4-6');
  });

  it('clicking Close on the detail dialog dismisses it', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);

    render(<History />);
    await screen.findByTestId('history-row');
    fireEvent.click(screen.getByTestId('history-view'));
    await screen.findByTestId('history-detail');

    fireEvent.click(screen.getByTestId('history-detail-close'));

    expect(screen.queryByTestId('history-detail')).not.toBeInTheDocument();
  });

  it('clicking Restore original on the detail dialog calls history_restore and closes it', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);
    mockedIpc.historyRestore.mockResolvedValue('we good with the release plan i think');

    render(<History />);
    await screen.findByTestId('history-row');
    fireEvent.click(screen.getByTestId('history-view'));
    await screen.findByTestId('history-detail');

    fireEvent.click(screen.getByTestId('history-detail-restore'));

    await waitFor(() => expect(mockedIpc.historyRestore).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(screen.queryByTestId('history-detail')).not.toBeInTheDocument());
  });

  // ── Copy (C12/S30) ──

  it('clicking Copy calls history_copy for that entry and writes the refined text to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1', refined: 'The refined text to copy.' })]);
    mockedIpc.historyCopy.mockResolvedValue(entry({ id: '1', refined: 'The refined text to copy.' }));

    render(<History />);
    await screen.findByTestId('history-row');

    fireEvent.click(screen.getByTestId('history-copy'));

    await waitFor(() => expect(mockedIpc.historyCopy).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('The refined text to copy.'));
  });

  it('clicking Copy refined on the detail dialog calls history_copy for that entry', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1', refined: 'Detail refined text.' })]);
    mockedIpc.historyCopy.mockResolvedValue(entry({ id: '1', refined: 'Detail refined text.' }));

    render(<History />);
    await screen.findByTestId('history-row');
    fireEvent.click(screen.getByTestId('history-view'));
    await screen.findByTestId('history-detail');

    fireEvent.click(screen.getByTestId('history-detail-copy'));

    await waitFor(() => expect(mockedIpc.historyCopy).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('Detail refined text.'));
  });

  // ── Clear all (C15/S33) ──

  it('clicking Clear history opens a confirmation dialog without clearing yet', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);

    render(<History />);
    await screen.findByTestId('history-row');

    fireEvent.click(screen.getByTestId('history-clear'));

    await screen.findByTestId('history-clear-modal');
    expect(mockedIpc.historyClear).not.toHaveBeenCalled();
    expect(screen.getByTestId('history-row')).toBeInTheDocument();
  });

  it('cancelling the clear confirmation dismisses it without clearing', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' })]);

    render(<History />);
    await screen.findByTestId('history-row');
    fireEvent.click(screen.getByTestId('history-clear'));
    await screen.findByTestId('history-clear-modal');

    fireEvent.click(screen.getByTestId('history-clear-cancel'));

    expect(screen.queryByTestId('history-clear-modal')).not.toBeInTheDocument();
    expect(mockedIpc.historyClear).not.toHaveBeenCalled();
    expect(screen.getByTestId('history-row')).toBeInTheDocument();
  });

  it('confirming clear calls history_clear and empties the list', async () => {
    mockedIpc.historyList.mockResolvedValue([entry({ id: '1' }), entry({ id: '2' })]);
    mockedIpc.historyClear.mockResolvedValue(undefined);

    render(<History />);
    await screen.findAllByTestId('history-row');
    fireEvent.click(screen.getByTestId('history-clear'));
    await screen.findByTestId('history-clear-modal');

    fireEvent.click(screen.getByTestId('history-clear-confirm'));

    await waitFor(() => expect(mockedIpc.historyClear).toHaveBeenCalled());
    await screen.findByTestId('history-empty');
    expect(screen.queryByTestId('history-row')).not.toBeInTheDocument();
  });
});
