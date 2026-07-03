import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Capture from './Capture';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  refine: vi.fn(),
  restoreOriginal: vi.fn(),
  injectText: vi.fn(),
  cancelRefine: vi.fn(),
  permissionOpenSettings: vi.fn(),
  trayQuit: vi.fn(),
  NO_ACTIVE_MODEL_ERROR: 'no_active_model',
  PERMISSION_DENIED_ERROR: 'permission_denied',
}));

const mockedIpc = vi.mocked(ipc);

describe('Capture', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('shows the capture panel with the editor and a Refine button by default', () => {
    render(<Capture />);

    expect(screen.getByTestId('capture-panel')).toBeInTheDocument();
    expect(screen.getByTestId('capture-editor')).toBeInTheDocument();
    expect(screen.getByTestId('capture-refine')).toBeInTheDocument();
    expect(screen.getByTestId('capture-active-model')).toHaveTextContent('No model selected');
  });

  it('refines the captured selection and shows the blind-injected result with a restore control', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
    });

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    await waitFor(() => expect(mockedIpc.refine).toHaveBeenCalledTimes(1));
    expect(await screen.findByText('polished draft')).toBeInTheDocument();
    expect(screen.getByTestId('capture-restore')).toBeInTheDocument();
    expect(screen.getByTestId('capture-active-model')).toHaveTextContent('claude-opus-4-6');
    // The blind inject is a single backend call — the frontend never calls
    // inject_text directly for the default (non-restore) refine path.
    expect(mockedIpc.injectText).not.toHaveBeenCalled();
  });

  it('restores the pre-refine original via restore_original + inject_text', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
    });
    mockedIpc.restoreOriginal.mockResolvedValue('rough draft');
    mockedIpc.injectText.mockResolvedValue(undefined);

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));
    await screen.findByTestId('capture-restore');

    fireEvent.click(screen.getByTestId('capture-restore'));

    await waitFor(() => expect(mockedIpc.restoreOriginal).toHaveBeenCalledTimes(1));
    expect(mockedIpc.injectText).toHaveBeenCalledWith('rough draft');
    expect(await screen.findByText('Restored original')).toBeInTheDocument();
    expect(screen.getByTestId('capture-restore')).toBeDisabled();
  });

  it('routes to choosing a model when refine fails with no_active_model', async () => {
    mockedIpc.refine.mockRejectedValue('no_active_model');

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    expect(await screen.findByTestId('capture-no-model-cta')).toBeInTheDocument();
  });

  it('shows the permission-needed state when refine fails with permission_denied', async () => {
    mockedIpc.refine.mockRejectedValue('permission_denied');

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    const openSettingsBtn = await screen.findByTestId('capture-perm-open-settings');
    fireEvent.click(openSettingsBtn);

    expect(mockedIpc.permissionOpenSettings).toHaveBeenCalledTimes(1);
  });

  it('shows an error state with a retry action on an unrecognized refine failure, leaving text untouched', async () => {
    mockedIpc.refine.mockRejectedValue(new Error('model_unreachable'));

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    const errorSection = await screen.findByTestId('capture-error');
    expect(errorSection).toHaveTextContent('model_unreachable');
    expect(screen.queryByTestId('capture-no-model')).not.toBeInTheDocument();
    expect(screen.queryByTestId('capture-permission')).not.toBeInTheDocument();
    // Nothing is injected on a generic failure — the user's text is untouched.
    expect(mockedIpc.injectText).not.toHaveBeenCalled();

    // Retry re-invokes the same refine pipeline (controls/capture.json:
    // capture-retry -> refine).
    mockedIpc.refine.mockResolvedValueOnce({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
    });
    fireEvent.click(screen.getByTestId('capture-retry'));

    await waitFor(() => expect(mockedIpc.refine).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('polished draft')).toBeInTheDocument();
  });

  it('shows a fallback-models indication in the error state when the rejection carries a fallback chain', async () => {
    mockedIpc.refine.mockRejectedValue({
      message: 'Ollama unreachable',
      fallbackModels: ['claude-sonnet-4-6', 'gpt-5.1'],
    });

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    const errorSection = await screen.findByTestId('capture-error');
    expect(errorSection).toHaveTextContent('Ollama unreachable');
    expect(errorSection).toHaveTextContent('claude-sonnet-4-6');
    expect(errorSection).toHaveTextContent('gpt-5.1');
  });

  it('renders the live command-parse preview for the captured selection', () => {
    render(<Capture capturedText="/rd keep it warm /m we're good, shipping Monday" />);

    const preview = screen.getByTestId('capture-preview');
    expect(preview).toHaveTextContent('/rd');
    expect(preview).toHaveTextContent('direction');
    expect(preview).toHaveTextContent('/m');
    expect(preview).toHaveTextContent('your message');
  });

  it('shows the default/no-tags preview chip when the captured selection has no tags', () => {
    render(<Capture capturedText="just a plain draft with no tags" />);

    const preview = screen.getByTestId('capture-preview');
    expect(preview).toHaveTextContent('default direction');
  });

  it('shows the review state for a pending-review refine result, and accepting it injects the refined text', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
      status: 'pending_review',
    });
    mockedIpc.injectText.mockResolvedValue(undefined);

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    const review = await screen.findByTestId('capture-review');
    expect(review).toHaveTextContent('rough draft');
    expect(review).toHaveTextContent('polished draft');
    expect(screen.queryByTestId('capture-done')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('capture-accept'));

    await waitFor(() => expect(mockedIpc.injectText).toHaveBeenCalledWith('polished draft'));
    expect(await screen.findByTestId('capture-done')).toBeInTheDocument();
  });

  it('lets the user edit the refined text before accepting it in review', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
      status: 'pending_review',
    });
    mockedIpc.injectText.mockResolvedValue(undefined);

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));
    await screen.findByTestId('capture-review');

    fireEvent.click(screen.getByTestId('capture-edit'));

    const field = await screen.findByTestId('capture-edit-field');
    expect(field).toHaveValue('polished draft');
    fireEvent.change(field, { target: { value: 'even more polished draft' } });

    fireEvent.click(screen.getByTestId('capture-edit-accept'));

    await waitFor(() => expect(mockedIpc.injectText).toHaveBeenCalledWith('even more polished draft'));
    expect(await screen.findByText('even more polished draft')).toBeInTheDocument();
  });

  it('cancels an in-progress edit back to the review state without injecting', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
      status: 'pending_review',
    });

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));
    await screen.findByTestId('capture-review');

    fireEvent.click(screen.getByTestId('capture-edit'));
    await screen.findByTestId('capture-edit-field');

    fireEvent.click(screen.getByTestId('capture-edit-cancel'));

    expect(await screen.findByTestId('capture-review')).toBeInTheDocument();
    expect(mockedIpc.injectText).not.toHaveBeenCalled();
  });

  it('shows the error state for a plain string rejection with no fallback chain', async () => {
    mockedIpc.refine.mockRejectedValue('model_unreachable');

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    const errorSection = await screen.findByTestId('capture-error');
    expect(errorSection).toHaveTextContent('model_unreachable');
    expect(screen.queryByTestId('capture-error-fallback')).not.toBeInTheDocument();
  });

  it('discards a pending review result via cancel_refine, leaving the original untouched', async () => {
    mockedIpc.refine.mockResolvedValue({
      original: 'rough draft',
      refined: 'polished draft',
      model: 'claude-opus-4-6',
      status: 'pending_review',
    });
    mockedIpc.cancelRefine.mockResolvedValue(undefined);

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));
    await screen.findByTestId('capture-review');

    fireEvent.click(screen.getByTestId('capture-discard'));

    await waitFor(() => expect(mockedIpc.cancelRefine).toHaveBeenCalledTimes(1));
    expect(mockedIpc.injectText).not.toHaveBeenCalled();
    expect(await screen.findByTestId('capture-refine')).toBeInTheDocument();
  });

  it('calls onDismiss when capture-dismiss is clicked', () => {
    const onDismiss = vi.fn();
    render(<Capture onDismiss={onDismiss} />);

    fireEvent.click(screen.getByTestId('capture-dismiss'));

    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('expands the display-only tray preview and quits via tray-quit', () => {
    render(<Capture />);

    expect(screen.queryByTestId('tray')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('tray-btn'));
    expect(screen.getByTestId('tray')).toBeInTheDocument();

    // The active-model row expands the (inert, display-only) model list.
    expect(screen.queryByTestId('tray-model-list')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('tray-active-model'));
    expect(screen.getByTestId('tray-model-list')).toBeInTheDocument();
    expect(screen.getByTestId('tray-fav-opus')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('tray-quit'));
    expect(mockedIpc.trayQuit).toHaveBeenCalledTimes(1);
  });
});
