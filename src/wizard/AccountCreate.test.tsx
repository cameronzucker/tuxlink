// AccountCreate.test.tsx — in-app account creation (tuxlink-vfb3 sub-project 1).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useEffect } from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { AccountCreate } from './AccountCreate';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

// AccountCreate opens the external winlink.org register link via tauri-plugin-shell in
// the keyless degraded state (tuxlink-6afw).
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }));
import { open as shellOpen } from '@tauri-apps/plugin-shell';

// Render AccountCreate on the real provider, seeded onto the account_create step.
function renderCreate() {
  render(
    <WizardProvider>
      <Seed />
      <AccountCreate />
      <Probe />
    </WizardProvider>
  );
}

function Seed() {
  const { dispatch } = useWizard();
  // Drive the reducer to account_create the production way: credentials → account_create.
  useEffect(() => {
    dispatch({ type: 'SET_CONNECT_TO_CMS', payload: true });
    dispatch({ type: 'ADVANCE_FROM_ACCOUNT' });
    dispatch({ type: 'GO_TO_ACCOUNT_CREATE' });
  }, [dispatch]);
  return null;
}

function Probe() {
  const { state } = useWizard();
  return (
    <>
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-callsign">{state.callsign}</div>
    </>
  );
}

function fillValid() {
  fireEvent.change(screen.getByLabelText('Callsign *'), { target: { value: 'KK7ABC' } });
  fireEvent.change(screen.getByLabelText('Password *'), { target: { value: 'r7-Granite' } });
  fireEvent.change(screen.getByTestId('wc-confirm'), { target: { value: 'r7-Granite' } });
  fireEvent.change(screen.getByTestId('wc-recovery'), { target: { value: 'kk7abc.ops@gmail.com' } });
}

beforeEach(() => {
  vi.clearAllMocks();
  // Default routed mock: in-app creation is AVAILABLE (access key present) so the form
  // renders; the create/persist commands succeed. Per-test overrides replace this. The
  // mount probe (cms_password_change_available) must be routed, not consumed by a
  // *Once mock, or it would steal a queued rejection (tuxlink-6afw).
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'cms_password_change_available') return true;
    return undefined;
  });
});

// invoke() call names with the always-present availability probe filtered out, so a
// test can assert the create/persist call sequence in isolation.
function invokeCmds(): string[] {
  return (invoke as ReturnType<typeof vi.fn>).mock.calls
    .map((c) => c[0] as string)
    .filter((c) => c !== 'cms_password_change_available');
}

describe('<AccountCreate>', () => {
  it('renders callsign, password, confirm, and mandatory recovery-email fields', async () => {
    renderCreate();
    expect(await screen.findByLabelText('Callsign *')).toBeInTheDocument();
    expect(screen.getByLabelText('Password *')).toBeInTheDocument();
    expect(screen.getByTestId('wc-confirm')).toBeInTheDocument();
    expect(screen.getByTestId('wc-recovery')).toBeInTheDocument();
  });

  it('keeps submit disabled until callsign, 6-12 password, match, and recovery email are valid', async () => {
    renderCreate();
    const submit = (await screen.findByTestId('wc-submit')) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    // Tactical callsign + missing recovery → still disabled.
    fireEvent.change(screen.getByLabelText('Callsign *'), { target: { value: 'RELAY1' } });
    fireEvent.change(screen.getByLabelText('Password *'), { target: { value: 'r7-Granite' } });
    fireEvent.change(screen.getByTestId('wc-confirm'), { target: { value: 'r7-Granite' } });
    expect(submit.disabled).toBe(true);
    // Fix the callsign + add recovery → enabled.
    fillValid();
    expect(submit.disabled).toBe(false);
  });

  it('on success invokes cms_account_create then wizard_persist_cms and advances to cms_verify', async () => {
    renderCreate();
    await screen.findByTestId('wc-submit');
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('cms_verify'));
    const calls = invokeCmds();
    expect(calls).toEqual(['cms_account_create', 'wizard_persist_cms']);
    // cms_account_create carried the recovery email (the probe call precedes it).
    const createArgs = (invoke as ReturnType<typeof vi.fn>).mock.calls.find(
      (c) => c[0] === 'cms_account_create',
    )?.[1];
    expect(createArgs).toMatchObject({ rawCallsign: 'KK7ABC', recoveryEmail: 'kk7abc.ops@gmail.com' });
  });

  it('shows the sign-in offer on a "callsign exists" rejection and returns to credentials', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'cms_password_change_available') return true;
      if (cmd === 'cms_account_create') {
        throw { kind: 'Rejected', code: 'CallsignExists', message: 'KK7ABC already has an account' };
      }
      return undefined;
    });
    renderCreate();
    await screen.findByTestId('wc-submit');
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() => expect(screen.getByTestId('wc-exists')).toBeInTheDocument());
    // wizard_persist_cms must NOT have run (creation failed).
    const calls = invokeCmds();
    expect(calls).toEqual(['cms_account_create']);
    // "Sign in with this callsign" returns to credentials with the callsign preserved.
    fireEvent.click(screen.getByText(/sign in with this callsign/i));
    expect(screen.getByTestId('probe-step').textContent).toBe('credentials');
    expect(screen.getByTestId('probe-callsign').textContent).toBe('KK7ABC');
  });

  it('surfaces a non-exists rejection message verbatim without leaving the step', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'cms_password_change_available') return true;
      if (cmd === 'cms_account_create') {
        throw { kind: 'Rejected', code: 'WeakPassword', message: 'Password does not meet requirements' };
      }
      return undefined;
    });
    renderCreate();
    await screen.findByTestId('wc-submit');
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() =>
      expect(screen.getByTestId('wc-error').textContent).toMatch(/does not meet requirements/i)
    );
    expect(screen.getByTestId('probe-step').textContent).toBe('account_create');
  });

  // tuxlink-6afw: keyless build (no TUXLINK_WINLINK_ACCESS_CODE) — the dialog is still
  // reachable (ungated) but degrades to an honest note + external winlink.org link
  // instead of a form that would fail on submit.
  it('degrades to a note + external winlink.org link when no access key is present', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'cms_password_change_available') return false;
      return undefined;
    });
    vi.mocked(shellOpen).mockResolvedValue(undefined);
    renderCreate();
    const link = await screen.findByTestId('wc-register-external');
    expect(screen.getByTestId('wc-unavailable')).toBeInTheDocument();
    // The working form is not offered in this state.
    expect(screen.queryByTestId('wc-submit')).not.toBeInTheDocument();
    // The link opens the system browser, never the create command.
    fireEvent.click(link);
    await waitFor(() =>
      expect(shellOpen).toHaveBeenCalledWith(expect.stringContaining('winlink.org')),
    );
    expect(invokeCmds()).toEqual([]); // no cms_account_create attempted
  });
});
