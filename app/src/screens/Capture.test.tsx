import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Capture from './Capture';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  refine: vi.fn(),
  restoreOriginal: vi.fn(),
  injectText: vi.fn(),
  permissionOpenSettings: vi.fn(),
  trayQuit: vi.fn(),
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

  it('returns to the input state, leaving text untouched, on an unrecognized refine failure', async () => {
    mockedIpc.refine.mockRejectedValue(new Error('model_unreachable'));

    render(<Capture />);
    fireEvent.click(screen.getByTestId('capture-refine'));

    await waitFor(() => expect(mockedIpc.refine).toHaveBeenCalledTimes(1));
    expect(await screen.findByTestId('capture-refine')).toBeInTheDocument();
    expect(screen.queryByTestId('capture-no-model')).not.toBeInTheDocument();
    expect(screen.queryByTestId('capture-permission')).not.toBeInTheDocument();
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
