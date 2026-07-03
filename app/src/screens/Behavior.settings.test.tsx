import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Behavior from './Behavior';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

describe('Behavior settings: inject mode, quote behavior, fallback chain', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.settingsGet.mockResolvedValue(null);
    mockedIpc.settingsSet.mockResolvedValue(undefined);
  });

  it('defaults inject mode to blind and switches to review on click, persisting via settings_set', async () => {
    render(<Behavior />);

    await screen.findByTestId('inject-mode');
    expect(screen.getByTestId('inject-mode-blind')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('inject-mode-review')).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(screen.getByTestId('inject-mode-review'));

    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.inject_mode', 'review'));
    expect(screen.getByTestId('inject-mode-review')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('inject-mode-blind')).toHaveAttribute('aria-checked', 'false');
  });

  it('restores a previously persisted review inject mode from settings_get', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'behavior.inject_mode' ? 'review' : null),
    );

    render(<Behavior />);

    await waitFor(() => expect(screen.getByTestId('inject-mode-review')).toHaveAttribute('aria-checked', 'true'));
    expect(screen.getByTestId('inject-mode-blind')).toHaveAttribute('aria-checked', 'false');
  });

  it('lets the user pick a quote-behavior mode and persists it via settings_set', async () => {
    render(<Behavior />);

    const answerQuote = await screen.findByTestId('quote-answer-quote');
    expect(answerQuote).toBeChecked();

    fireEvent.click(screen.getByTestId('quote-answer'));
    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.quote_mode', 'answer'));
    expect(screen.getByTestId('quote-answer')).toBeChecked();

    fireEvent.click(screen.getByTestId('quote-rd'));
    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.quote_mode', 'rd'));
    expect(screen.getByTestId('quote-rd')).toBeChecked();
  });

  it('restores a previously persisted quote mode from settings_get', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'behavior.quote_mode' ? 'rd' : null),
    );

    render(<Behavior />);

    await waitFor(() => expect(screen.getByTestId('quote-rd')).toBeChecked());
    expect(screen.getByTestId('quote-answer-quote')).not.toBeChecked();
  });

  it('defaults on-failure to notify and switches to fallback on click, persisting via settings_set', async () => {
    render(<Behavior />);

    const notify = await screen.findByTestId('failure-notify');
    expect(notify).toBeChecked();

    fireEvent.click(screen.getByTestId('failure-fallback'));
    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.on_failure', 'fallback'));
    expect(screen.getByTestId('failure-fallback')).toBeChecked();
  });

  it('starts with a two-model fallback chain and persists a change to the first model', async () => {
    render(<Behavior />);

    const model1 = await screen.findByTestId('failure-fallback-model-1');
    const model2 = screen.getByTestId('failure-fallback-model-2');
    expect(model1).toBeInTheDocument();
    expect(model2).toBeInTheDocument();

    fireEvent.change(model1, { target: { value: 'claude-opus-4-6' } });

    await waitFor(() =>
      expect(mockedIpc.settingsSet).toHaveBeenCalledWith(
        'behavior.fallback_chain',
        JSON.stringify(['claude-opus-4-6', 'qwen3:8b']),
      ),
    );
  });

  it('removes a fallback model from the chain and persists the shortened chain', async () => {
    render(<Behavior />);

    await screen.findByTestId('failure-fallback-model-2');
    fireEvent.click(screen.getByTestId('failure-fallback-remove-1'));

    await waitFor(() =>
      expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.fallback_chain', JSON.stringify(['qwen3:8b'])),
    );
    expect(screen.queryByTestId('failure-fallback-model-2')).not.toBeInTheDocument();
    expect(screen.getByTestId('failure-fallback-model-1')).toBeInTheDocument();
  });

  it('adds another fallback model to the chain and persists the lengthened chain', async () => {
    render(<Behavior />);

    await screen.findByTestId('failure-fallback-model-2');
    fireEvent.click(screen.getByTestId('failure-fallback-add'));

    await waitFor(() => expect(screen.getByTestId('failure-fallback-model-3')).toBeInTheDocument());
    expect(mockedIpc.settingsSet).toHaveBeenCalledWith(
      'behavior.fallback_chain',
      JSON.stringify(['gpt-5.1', 'qwen3:8b', 'claude-opus-4-6']),
    );
  });

  it('restores a previously persisted fallback chain from settings_get', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'behavior.fallback_chain' ? JSON.stringify(['gemini-1.5-flash']) : null),
    );

    render(<Behavior />);

    await waitFor(() => expect(screen.getByTestId('failure-fallback-model-1')).toHaveValue('gemini-1.5-flash'));
    expect(screen.queryByTestId('failure-fallback-model-2')).not.toBeInTheDocument();
  });

  it('changes and persists the retry count', async () => {
    render(<Behavior />);

    const retryCount = await screen.findByTestId('failure-retry-count');
    expect(retryCount).toHaveValue('2');

    fireEvent.change(retryCount, { target: { value: '3' } });

    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('behavior.retry_count', '3'));
    expect(retryCount).toHaveValue('3');
  });
});
