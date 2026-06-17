// WinlinkAccountSettings tests (tuxlink-vfb3). The Settings "Winlink Account"
// section: hosts the CMS password-CHANGE control and a keyring-only "re-enter
// password" recovery form. Re-enter writes ONLY the keyring (credentials_write_
// password) for the active identity — it never rewrites config.json.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// CmsPasswordChange has its own tests + availability gate; stub it so this test
// asserts the section's own wiring (and the callsign it forwards).
vi.mock('../wizard/CmsPasswordChange', () => ({
  CmsPasswordChange: ({ callsign }: { callsign: string }) => (
    <div data-testid="cms-password-change-stub">{callsign}</div>
  ),
}));

import { invoke } from '@tauri-apps/api/core';
import { WinlinkAccountSettings } from './WinlinkAccountSettings';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** Route invoke: identity_active + credentials_write_password. */
function route(opts: { active?: unknown; write?: () => Promise<unknown> } = {}) {
  const active =
    'active' in opts ? opts.active : { mycall: 'N7CPZ', address_as: 'N7CPZ', is_tactical: false };
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'identity_active') return Promise.resolve(active);
    if (cmd === 'credentials_write_password') {
      return opts.write ? opts.write() : Promise.resolve(undefined);
    }
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('<WinlinkAccountSettings>', () => {
  it('shows the active account callsign', async () => {
    route();
    render(<WinlinkAccountSettings />);
    expect(await screen.findByTestId('account-current-callsign')).toHaveTextContent('N7CPZ');
  });

  it('forwards the active callsign to the password-change control', async () => {
    route({ active: { mycall: 'K7ABC', address_as: 'K7ABC', is_tactical: false } });
    render(<WinlinkAccountSettings />);
    const stub = await screen.findByTestId('cms-password-change-stub');
    expect(stub).toHaveTextContent('K7ABC');
  });

  it('re-enter: writes the password to the keyring for the active identity', async () => {
    route({ active: { mycall: 'K7ABC', address_as: 'K7ABC', is_tactical: false } });
    render(<WinlinkAccountSettings />);
    await screen.findByTestId('account-reenter-password');
    fireEvent.change(screen.getByTestId('account-reenter-password'), {
      target: { value: 'restored-pw' },
    });
    fireEvent.click(screen.getByTestId('account-reenter-submit'));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([c]) => c === 'credentials_write_password');
      expect(call).toBeTruthy();
      expect(call?.[1]).toEqual({ callsign: 'K7ABC', password: 'restored-pw' });
    });
    expect(await screen.findByTestId('account-reenter-success')).toBeInTheDocument();
    // Field cleared after success (no lingering secret in the DOM).
    expect((screen.getByTestId('account-reenter-password') as HTMLInputElement).value).toBe('');
  });

  it('re-enter: submit is disabled until the password is valid (>=6)', async () => {
    route();
    render(<WinlinkAccountSettings />);
    await screen.findByTestId('account-reenter-submit');
    const submit = screen.getByTestId('account-reenter-submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-reenter-password'), { target: { value: 'short' } });
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-reenter-password'), {
      target: { value: 'longenough' },
    });
    expect(submit.disabled).toBe(false);
  });

  it('re-enter: surfaces a failure and shows no success', async () => {
    route({ write: () => Promise.reject(new Error('keyring locked')) });
    render(<WinlinkAccountSettings />);
    await screen.findByTestId('account-reenter-password');
    fireEvent.change(screen.getByTestId('account-reenter-password'), {
      target: { value: 'longenough' },
    });
    fireEvent.click(screen.getByTestId('account-reenter-submit'));
    expect(await screen.findByTestId('account-reenter-error')).toBeInTheDocument();
    expect(screen.queryByTestId('account-reenter-success')).toBeNull();
  });

  it('shows a no-account message and no controls when there is no active identity', async () => {
    route({ active: null });
    render(<WinlinkAccountSettings />);
    expect(await screen.findByTestId('account-none')).toBeInTheDocument();
    expect(screen.queryByTestId('cms-password-change-stub')).toBeNull();
    expect(screen.queryByTestId('account-reenter-password')).toBeNull();
  });
});
