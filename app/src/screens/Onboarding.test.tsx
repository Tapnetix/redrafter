import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import Onboarding from './Onboarding';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

describe('Onboarding', () => {
  afterEach(() => {
    mockInvoke.mockReset();
  });

  it('blocks Continue and shows "Not granted" while Accessibility is ungranted', async () => {
    mockInvoke.mockResolvedValue({ granted: false });

    render(<Onboarding />);

    expect(await screen.findByText('Not granted')).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith('permission_status');
    expect(screen.getByTestId('perm-status')).toHaveAttribute('data-granted', 'false');

    const continueBtn = screen.getByTestId('perm-continue');
    expect(continueBtn).toBeDisabled();
  });

  it('calls permission_open_settings when "Open System Settings" is clicked', async () => {
    mockInvoke.mockResolvedValue({ granted: false });

    render(<Onboarding />);
    await screen.findByText('Not granted');

    fireEvent.click(screen.getByTestId('perm-open-settings'));

    expect(mockInvoke).toHaveBeenCalledWith('permission_open_settings');
  });

  it('enables Continue and invokes onContinue once Accessibility is granted', async () => {
    mockInvoke.mockResolvedValue({ granted: true });
    const onContinue = vi.fn();

    render(<Onboarding onContinue={onContinue} />);

    await waitFor(() => {
      expect(screen.getByTestId('perm-status')).toHaveAttribute('data-granted', 'true');
    });
    expect(await screen.findByText('Granted')).toBeInTheDocument();

    const continueBtn = screen.getByTestId('perm-continue');
    expect(continueBtn).toBeEnabled();

    fireEvent.click(continueBtn);
    expect(onContinue).toHaveBeenCalledTimes(1);
  });

  it('does not invoke onContinue when Continue is clicked while ungranted', async () => {
    mockInvoke.mockResolvedValue({ granted: false });
    const onContinue = vi.fn();

    render(<Onboarding onContinue={onContinue} />);
    await screen.findByText('Not granted');

    fireEvent.click(screen.getByTestId('perm-continue'));
    expect(onContinue).not.toHaveBeenCalled();
  });
});
