import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import FirstRun from './FirstRun';
import { connectionAdd } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  connectionAdd: vi.fn(),
}));

const connectionAddMock = vi.mocked(connectionAdd);

describe('FirstRun', () => {
  beforeEach(() => {
    connectionAddMock.mockReset();
  });

  it('defaults to the Cloud tab with Anthropic selected', () => {
    render(<FirstRun />);

    expect(screen.getByTestId('firstrun-cloud')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('firstrun-cloud-panel')).not.toHaveAttribute('hidden');
    expect(screen.getByTestId('firstrun-provider-anthropic')).toHaveAttribute('aria-checked', 'true');
  });

  it('connects the selected cloud provider via connection_add and shows Connected', async () => {
    connectionAddMock.mockResolvedValue({
      id: '1',
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      enabledModels: ['claude-3-5-sonnet-latest'],
    });

    render(<FirstRun />);

    fireEvent.change(screen.getByTestId('firstrun-key'), { target: { value: 'sk-ant-test' } });
    fireEvent.click(screen.getByTestId('firstrun-connect'));

    await waitFor(() => expect(screen.getByTestId('firstrun-connect')).toHaveTextContent('Connected'));

    expect(connectionAddMock).toHaveBeenCalledWith({
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      apiKey: 'sk-ant-test',
    });
  });

  it('shows an inline error and stays connectable again when connection_add rejects', async () => {
    connectionAddMock.mockRejectedValue(new Error('invalid API key'));

    render(<FirstRun />);

    fireEvent.change(screen.getByTestId('firstrun-key'), { target: { value: 'bad-key' } });
    fireEvent.click(screen.getByTestId('firstrun-connect'));

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('invalid API key'));
    expect(screen.getByTestId('firstrun-connect')).toHaveTextContent('Connect');
  });

  it('switches to the Local tab and connects the detected Ollama endpoint', async () => {
    connectionAddMock.mockResolvedValue({
      id: '2',
      providerKind: 'ollama',
      baseUrl: 'http://localhost:11434',
      enabledModels: ['default'],
    });

    render(<FirstRun />);

    fireEvent.click(screen.getByTestId('firstrun-local'));

    expect(screen.getByTestId('firstrun-local-panel')).not.toHaveAttribute('hidden');
    expect(screen.getByTestId('firstrun-ollama-detected')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('firstrun-ollama-connect'));

    await waitFor(() =>
      expect(screen.getByTestId('firstrun-ollama-connect')).toHaveTextContent('Connected'),
    );
    expect(connectionAddMock).toHaveBeenCalledWith({
      providerKind: 'ollama',
      baseUrl: 'http://localhost:11434',
      apiKey: '',
    });
  });

  it('calls onContinue when Continue is clicked', () => {
    const onContinue = vi.fn();
    render(<FirstRun onContinue={onContinue} />);

    fireEvent.click(screen.getByTestId('firstrun-continue'));

    expect(onContinue).toHaveBeenCalledTimes(1);
  });

  it('switches the selected cloud provider and resets connect status, and can switch tabs back to Cloud', async () => {
    connectionAddMock.mockResolvedValue({
      id: '1',
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      enabledModels: ['claude-3-5-sonnet-latest'],
    });

    render(<FirstRun />);

    fireEvent.change(screen.getByTestId('firstrun-key'), { target: { value: 'sk-ant-test' } });
    fireEvent.click(screen.getByTestId('firstrun-connect'));
    await waitFor(() => expect(screen.getByTestId('firstrun-connect')).toHaveTextContent('Connected'));

    fireEvent.click(screen.getByTestId('firstrun-provider-openai'));

    expect(screen.getByTestId('firstrun-provider-openai')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('firstrun-provider-anthropic')).toHaveAttribute('aria-checked', 'false');
    // Selecting a different provider resets the connect button back to idle.
    expect(screen.getByTestId('firstrun-connect')).toHaveTextContent('Connect');

    fireEvent.click(screen.getByTestId('firstrun-local'));
    expect(screen.getByTestId('firstrun-local-panel')).not.toHaveAttribute('hidden');
    fireEvent.click(screen.getByTestId('firstrun-cloud'));
    expect(screen.getByTestId('firstrun-cloud-panel')).not.toHaveAttribute('hidden');
  });

  it('shows an inline error for the local Ollama connect failure', async () => {
    connectionAddMock.mockRejectedValue(new Error('ollama unreachable'));

    render(<FirstRun />);

    fireEvent.click(screen.getByTestId('firstrun-local'));
    fireEvent.click(screen.getByTestId('firstrun-ollama-connect'));

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('ollama unreachable'));
  });
});
