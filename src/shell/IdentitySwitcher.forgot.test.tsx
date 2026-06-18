// IdentitySwitcher forgot-password recovery (tuxlink-vfb3 sub-project 2).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { IdentitySwitcher } from './IdentitySwitcher';
import type { IdentityListDto } from './identityTypes';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const LIST: IdentityListDto = {
  full: [
    { callsign: 'W7XYZ', label: null, has_cms_account: true, cms_registered: true, needs_auth: false },
    { callsign: 'W1ABC', label: 'Club', has_cms_account: true, cms_registered: true, needs_auth: true },
  ],
  tactical: [],
  last_selected: 'W7XYZ',
};

function route(opts: { available?: boolean; send?: () => Promise<unknown> } = {}) {
  const available = opts.available ?? true;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'cms_password_change_available') return Promise.resolve(available);
    if (cmd === 'cms_account_send_recovery') return opts.send ? opts.send() : Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
}

/** Open the dropdown and reveal the unlock form for W1ABC. */
async function openUnlock() {
  fireEvent.click(screen.getByTestId('identity-switcher-trigger'));
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  return screen.findByTestId('identity-unlock');
}

beforeEach(() => invokeMock.mockReset());

describe('IdentitySwitcher forgot-password', () => {
  it('sends the recovery email for the FULL account and shows success', async () => {
    route();
    render(<IdentitySwitcher active={null} list={LIST} onSwitch={vi.fn().mockResolvedValue(undefined)} />);
    await openUnlock();
    const forgot = await screen.findByTestId('identity-forgot');
    fireEvent.click(forgot);
    await waitFor(() => expect(screen.getByTestId('identity-recovery-msg')).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith('cms_account_send_recovery', { rawCallsign: 'W1ABC' });
    expect(screen.getByTestId('identity-recovery-msg').textContent).toMatch(/emailed to the recovery address/i);
  });

  it('on "no recovery address on file" surfaces the server message + where to set one', async () => {
    route({ send: () => Promise.reject({ kind: 'Rejected', message: 'No recovery address is set.' }) });
    render(<IdentitySwitcher active={null} list={LIST} onSwitch={vi.fn().mockResolvedValue(undefined)} />);
    await openUnlock();
    fireEvent.click(await screen.findByTestId('identity-forgot'));
    await waitFor(() => expect(screen.getByTestId('identity-recovery-msg')).toBeInTheDocument());
    expect(screen.getByTestId('identity-recovery-msg').textContent).toMatch(/no recovery address is set/i);
    expect(screen.getByTestId('identity-recovery-msg').textContent).toMatch(/Settings → Winlink Account/i);
  });

  it('hides the forgot-password affordance when the account API is unavailable', async () => {
    route({ available: false });
    render(<IdentitySwitcher active={null} list={LIST} onSwitch={vi.fn().mockResolvedValue(undefined)} />);
    await openUnlock();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('cms_password_change_available'));
    expect(screen.queryByTestId('identity-forgot')).not.toBeInTheDocument();
  });
});
