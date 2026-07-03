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
});
