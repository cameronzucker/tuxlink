// AccountCreate.test.tsx — in-app account creation (tuxlink-vfb3 sub-project 1).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useEffect } from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { AccountCreate } from './AccountCreate';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

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

beforeEach(() => vi.clearAllMocks());

describe('<AccountCreate>', () => {
  it('renders callsign, password, confirm, and mandatory recovery-email fields', () => {
    renderCreate();
    expect(screen.getByLabelText('Callsign *')).toBeInTheDocument();
    expect(screen.getByLabelText('Password *')).toBeInTheDocument();
    expect(screen.getByTestId('wc-confirm')).toBeInTheDocument();
    expect(screen.getByTestId('wc-recovery')).toBeInTheDocument();
  });

  it('keeps submit disabled until callsign, 6-12 password, match, and recovery email are valid', () => {
    renderCreate();
    const submit = screen.getByTestId('wc-submit') as HTMLButtonElement;
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
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    renderCreate();
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('cms_verify'));
    const calls = (invoke as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
    expect(calls).toEqual(['cms_account_create', 'wizard_persist_cms']);
    // cms_account_create carried the recovery email.
    const createArgs = (invoke as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(createArgs).toMatchObject({ rawCallsign: 'KK7ABC', recoveryEmail: 'kk7abc.ops@gmail.com' });
  });

  it('shows the sign-in offer on a "callsign exists" rejection and returns to credentials', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce({
      kind: 'Rejected',
      code: 'CallsignExists',
      message: 'KK7ABC already has an account',
    });
    renderCreate();
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() => expect(screen.getByTestId('wc-exists')).toBeInTheDocument());
    // wizard_persist_cms must NOT have run (creation failed).
    const calls = (invoke as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
    expect(calls).toEqual(['cms_account_create']);
    // "Sign in with this callsign" returns to credentials with the callsign preserved.
    fireEvent.click(screen.getByText(/sign in with this callsign/i));
    expect(screen.getByTestId('probe-step').textContent).toBe('credentials');
    expect(screen.getByTestId('probe-callsign').textContent).toBe('KK7ABC');
  });

  it('surfaces a non-exists rejection message verbatim without leaving the step', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce({
      kind: 'Rejected',
      code: 'WeakPassword',
      message: 'Password does not meet requirements',
    });
    renderCreate();
    fillValid();
    fireEvent.click(screen.getByTestId('wc-submit'));
    await waitFor(() =>
      expect(screen.getByTestId('wc-error').textContent).toMatch(/does not meet requirements/i)
    );
    expect(screen.getByTestId('probe-step').textContent).toBe('account_create');
  });
});
