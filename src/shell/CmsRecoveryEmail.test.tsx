// CmsRecoveryEmail tests (tuxlink-vfb3 sub-project 3).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CmsRecoveryEmail } from './CmsRecoveryEmail';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function route(opts: { available?: boolean; set?: () => Promise<unknown> } = {}) {
  const available = opts.available ?? true;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'cms_password_change_available') return Promise.resolve(available);
    if (cmd === 'cms_account_set_recovery_email') return opts.set ? opts.set() : Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => invokeMock.mockReset());

describe('<CmsRecoveryEmail>', () => {
  it('renders nothing when unavailable', async () => {
    route({ available: false });
    const { container } = render(<CmsRecoveryEmail callsign="KK7ABC" />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('cms_password_change_available'));
    expect(container.querySelector('[data-testid="account-recovery-email"]')).toBeNull();
  });

  it('gates submit until a valid email AND the current password are present', async () => {
    route();
    render(<CmsRecoveryEmail callsign="KK7ABC" />);
    const submit = (await screen.findByTestId('account-recovery-submit')) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-recovery-new'), { target: { value: 'not-an-email' } });
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-recovery-new'), { target: { value: 'new@example.com' } });
    expect(submit.disabled).toBe(true); // still needs the password
    fireEvent.change(screen.getByTestId('account-recovery-password'), { target: { value: 'currentpw' } });
    expect(submit.disabled).toBe(false);
  });

  it('on success invokes cms_account_set_recovery_email with the proof password and shows success', async () => {
    route();
    render(<CmsRecoveryEmail callsign="KK7ABC" />);
    fireEvent.change(await screen.findByTestId('account-recovery-new'), { target: { value: 'new@example.com' } });
    fireEvent.change(screen.getByTestId('account-recovery-password'), { target: { value: 'currentpw' } });
    fireEvent.click(screen.getByTestId('account-recovery-submit'));
    await waitFor(() => expect(screen.getByTestId('account-recovery-success')).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith('cms_account_set_recovery_email', {
      rawCallsign: 'KK7ABC',
      password: 'currentpw',
      recoveryEmail: 'new@example.com',
    });
  });

  it('surfaces a rejection (e.g. wrong password) verbatim', async () => {
    route({ set: () => Promise.reject({ kind: 'Rejected', code: 'AUTH', message: 'Old password is incorrect' }) });
    render(<CmsRecoveryEmail callsign="KK7ABC" />);
    fireEvent.change(await screen.findByTestId('account-recovery-new'), { target: { value: 'new@example.com' } });
    fireEvent.change(screen.getByTestId('account-recovery-password'), { target: { value: 'wrongpw' } });
    fireEvent.click(screen.getByTestId('account-recovery-submit'));
    await waitFor(() =>
      expect(screen.getByTestId('account-recovery-error').textContent).toMatch(/incorrect/i)
    );
  });
});
