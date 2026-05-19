import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

// AppShell issues a `mailbox_list` invoke on mount (in addition to App's
// `get_wizard_completed`); route by command so each returns a shape-correct
// value. Without this, a single blanket resolved-value would feed a boolean
// to the message list or an array to the wizard router.
function routeInvoke(wizardCompleted: boolean) {
  (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
    if (cmd === 'get_wizard_completed') return Promise.resolve(wizardCompleted);
    if (cmd === 'mailbox_list') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

describe('<App>', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders wizard when wizard_completed=false', async () => {
    routeInvoke(false);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });

  it('renders main shell when wizard_completed=true', async () => {
    routeInvoke(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('app-shell-root')).toBeInTheDocument());
  });

  it('falls back to wizard when get_wizard_completed rejects', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('no config'));
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });
});
