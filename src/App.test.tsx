import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

describe('<App>', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders wizard when wizard_completed=false', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(false);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });

  it('renders main shell when wizard_completed=true', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('main-shell-root')).toBeInTheDocument());
  });

  it('falls back to wizard when get_wizard_completed rejects', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('no config'));
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });
});
