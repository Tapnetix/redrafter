import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import OnboardingPage from './page';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

describe('OnboardingPage', () => {
  afterEach(() => {
    mockInvoke.mockReset();
  });

  it('swaps the gate for a "continuing" marker once Continue is activated', async () => {
    mockInvoke.mockResolvedValue({ granted: true });

    render(<OnboardingPage />);

    const continueBtn = await screen.findByTestId('perm-continue');
    fireEvent.click(continueBtn);

    expect(screen.getByTestId('onboarding-continued')).toBeInTheDocument();
    expect(screen.queryByTestId('perm-status')).not.toBeInTheDocument();
  });
});
