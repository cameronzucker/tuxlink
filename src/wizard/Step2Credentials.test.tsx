// Step2Credentials.test.tsx — Task 3.3 / tuxlink-1r5
// Spec: §3.3 (Step 2-CMS behavior), §3.5 (error UX), §3.7 (shell-open for Register link)
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useEffect } from 'react';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { Step2Credentials } from './Step2Credentials';

// ── Mock Tauri invoke ──────────────────────────────────────────────────────
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

// ── Mock tauri-plugin-shell opener (for Register link) ────────────────────
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));
import { open as shellOpen } from '@tauri-apps/plugin-shell';

// ── Helper: render Step2Credentials inside WizardProvider ────────────────
function renderStep2() {
  render(
    <WizardProvider>
      <StepWithProbe />
    </WizardProvider>
  );
}

function StepWithProbe() {
  const { state } = useWizard();
  return (
    <>
      <Step2Credentials />
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-inflight">{String(state.inFlight)}</div>
      <div data-testid="probe-password">{state.password}</div>
    </>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  // Default: invoke resolves undefined. Covers the on-mount cms_password_change_available
  // availability probe (tuxlink-vfb3); when it resolves falsy the create affordance shows
  // the external "Register on winlink.org" link. Tests override per-case.
  (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
});

// ──────────────────────────────────────────────────────────────────────────
// Rendering
// ──────────────────────────────────────────────────────────────────────────

describe('<Step2Credentials>', () => {
  it('renders callsign, password, and MBO fields (grid moved to the Location step)', () => {
    renderStep2();
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/cms password/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/mbo address/i)).toBeInTheDocument();
    // Grid is no longer collected here — tuxlink-9xy1 moved it to StepLocation.
    expect(screen.queryByLabelText(/grid/i)).not.toBeInTheDocument();
  });

  it('renders Continue and Save-and-skip buttons', () => {
    renderStep2();
    expect(screen.getByRole('button', { name: /continue/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /save.*skip/i })).toBeInTheDocument();
  });

  it('renders a Register link', () => {
    renderStep2();
    expect(screen.getByRole('link', { name: /register/i })).toBeInTheDocument();
  });

  it('password field is initially type="password" (masked)', () => {
    renderStep2();
    const pw = screen.getByLabelText(/cms password/i) as HTMLInputElement;
    expect(pw.type).toBe('password');
  });

  it('show/hide toggle changes password field type', () => {
    renderStep2();
    const toggle = screen.getByRole('button', { name: /conceal|reveal/i });
    fireEvent.click(toggle);
    const pw = screen.getByLabelText(/cms password/i) as HTMLInputElement;
    expect(pw.type).toBe('text');
    fireEvent.click(toggle);
    expect((screen.getByLabelText(/cms password/i) as HTMLInputElement).type).toBe('password');
  });

  // ── Validation gates ────────────────────────────────────────────────────

  it('Continue is disabled when callsign is empty', () => {
    renderStep2();
    const btn = screen.getByRole('button', { name: /continue/i });
    expect(btn).toBeDisabled();
  });

  it('Continue is disabled when callsign valid but password empty', () => {
    renderStep2();
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    expect(screen.getByRole('button', { name: /continue/i })).toBeDisabled();
  });

  it('Continue is enabled when callsign and password are valid', () => {
    renderStep2();
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'password123' } });
    expect(screen.getByRole('button', { name: /continue/i })).not.toBeDisabled();
  });

  it('callsign inline error shows for non-ASCII input', () => {
    renderStep2();
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHSА' } }); // Cyrillic А
    // Trigger blur or change to show inline error
    fireEvent.blur(screen.getByLabelText(/callsign/i));
    expect(screen.getByRole('alert')).toBeInTheDocument();
  });

  // ── MBO auto-fill ───────────────────────────────────────────────────────

  it('MBO auto-fills <callsign>@winlink.org on callsign input when MBO is empty', () => {
    renderStep2();
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    const mboInput = screen.getByLabelText(/mbo address/i) as HTMLInputElement;
    expect(mboInput.value).toBe('W4PHS@winlink.org');
  });

  it('MBO does NOT auto-fill when operator has customized it', () => {
    renderStep2();
    fireEvent.change(screen.getByLabelText(/mbo address/i), { target: { value: 'CUSTOM@winlink.org' } });
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    const mboInput = screen.getByLabelText(/mbo address/i) as HTMLInputElement;
    expect(mboInput.value).toBe('CUSTOM@winlink.org');
  });

  // ── Submit — Continue (→ cms_verify) ─────────────────────────────────

  it('Continue calls invoke("wizard_persist_cms") and transitions to cms_verify on success', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_persist_cms', expect.objectContaining({
        rawCallsign: 'W4PHS',
        password: 'p@ssw0rd',
      }));
    });

    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('cms_verify');
    });
  });

  // ── Submit — Save-and-skip (→ location, then complete) ─────────────────

  it('Save-and-skip calls invoke and transitions to the Location step (tuxlink-9xy1)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });
    fireEvent.click(screen.getByRole('button', { name: /save.*skip/i }));

    await waitFor(() => {
      // skip-verify now lands on the Location step (not straight to complete).
      expect(screen.getByTestId('probe-step')).toHaveTextContent('location');
    });
  });

  // ── Password cleared from state after success ──────────────────────────

  it('password is cleared from WizardState after successful submit (spec §3.1 invariant 1)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));

    await waitFor(() => {
      // After SUBMIT_CREDENTIALS_SUCCESS, state.password must be cleared.
      expect(screen.getByTestId('probe-password')).toHaveTextContent('');
    });
  });

  // ── Error UX — ErrUnavailable ──────────────────────────────────────────

  it('shows ErrUnavailable message when invoke rejects with Unavailable', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({ kind: 'Unavailable' });
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/keyring|secret.service/i);
    });
  });

  // ── Error UX — ErrLocked ──────────────────────────────────────────────

  it('shows ErrLocked message and Retry button when keyring is locked', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({ kind: 'Locked' });
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/locked/i);
      expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
    });
  });

  // ── Register link opens system browser ─────────────────────────────────

  it('Register link click calls tauri-plugin-shell open() instead of navigating', async () => {
    (shellOpen as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    renderStep2();

    const link = screen.getByRole('link', { name: /register/i });
    fireEvent.click(link);

    await waitFor(() => {
      expect(shellOpen).toHaveBeenCalledWith(expect.stringContaining('winlink.org'));
    });
  });

  // ── Buttons disabled during inFlight ──────────────────────────────────

  it('buttons are disabled while inFlight=true', async () => {
    // Make invoke hang so we can check intermediate disabled state.
    let resolveInvoke: () => void;
    const hangingPromise = new Promise<void>(resolve => { resolveInvoke = resolve; });
    (invoke as ReturnType<typeof vi.fn>).mockReturnValue(hangingPromise);
    renderStep2();

    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'W4PHS' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'p@ssw0rd' } });

    act(() => { fireEvent.click(screen.getByRole('button', { name: /continue/i })); });

    await waitFor(() => {
      expect(screen.getByTestId('probe-inflight')).toHaveTextContent('true');
      expect(screen.getByRole('button', { name: /continue/i })).toBeDisabled();
      expect(screen.getByRole('button', { name: /save.*skip/i })).toBeDisabled();
    });

    // Resolve the hanging promise to let teardown proceed cleanly.
    act(() => { resolveInvoke!(); });
  });

  // ── Create-account affordance (tuxlink-vfb3 sub-project 1) ──────────────
  it('shows the in-app "Create a Winlink account" button when the feature is available', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(true); // cms_password_change_available → true
    renderStep2();
    await waitFor(() => expect(screen.getByTestId('cred-create-account')).toBeInTheDocument());
    // The external register fallback is absent in this mode.
    expect(screen.queryByTestId('cred-register-external')).not.toBeInTheDocument();
  });

  it('clicking "Create a Winlink account" advances to the account_create step', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(true);
    // In production Step2Credentials only renders on the `credentials` step (and
    // GO_TO_ACCOUNT_CREATE is a strict no-op elsewhere), so seed the wizard there.
    function SeedCreds() {
      const { dispatch } = useWizard();
      useEffect(() => {
        dispatch({ type: 'SET_CONNECT_TO_CMS', payload: true });
        dispatch({ type: 'ADVANCE_FROM_ACCOUNT' });
      }, [dispatch]);
      return null;
    }
    render(
      <WizardProvider>
        <SeedCreds />
        <StepWithProbe />
      </WizardProvider>
    );
    const btn = await screen.findByTestId('cred-create-account');
    act(() => { fireEvent.click(btn); });
    await waitFor(() => expect(screen.getByTestId('probe-step')).toHaveTextContent('account_create'));
  });

  it('degrades to the external winlink.org register link when the feature is unavailable', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(false); // not configured on this build
    renderStep2();
    await waitFor(() => expect(screen.getByTestId('cred-register-external')).toBeInTheDocument());
    expect(screen.queryByTestId('cred-create-account')).not.toBeInTheDocument();
  });
});
