// Step2OfflineIdentity.test.tsx — Phase 4 Task 4.1 (+ tuxlink-9xy1)
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.3 (Step 2-offline) + §5.4
//
// Tests: blank-submit allowed, identifier accepts tactical strings,
//        SUBMIT_OFFLINE_SUCCESS routes to the Location step (tuxlink-9xy1 moved grid
//        out of this step into StepLocation), Busy returned silently, inFlight
//        disables submit.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { Step2OfflineIdentity } from './Step2OfflineIdentity';

// ── Tauri mock ─────────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

// ── Step probe (reads the current step after a successful submit) ──────────

function StepProbe() {
  const { state } = useWizard();
  return (
    <>
      <Step2OfflineIdentity />
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-inflight">{String(state.inFlight)}</div>
      <div data-testid="probe-identifier">{state.identifier}</div>
    </>
  );
}

describe('<Step2OfflineIdentity>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
  });

  // ── Render ──────────────────────────────────────────────────────────────

  it('renders the screen title', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    expect(screen.getByRole('heading')).toBeInTheDocument();
  });

  it('renders the identifier field + a continue button (grid moved to the Location step)', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    expect(screen.getByLabelText(/station identifier/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /continue offline/i })).toBeInTheDocument();
    // Grid is no longer collected here — tuxlink-9xy1 moved it to StepLocation.
    expect(screen.queryByLabelText(/grid locator/i)).not.toBeInTheDocument();
  });

  it('renders the "optional" footer copy', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    expect(screen.getByText(/optional\./i)).toBeInTheDocument();
  });

  // ── Field behaviour ─────────────────────────────────────────────────────

  it('identifier field accepts a free-form tactical string', () => {
    render(<WizardProvider><StepProbe /></WizardProvider>);
    const field = screen.getByLabelText(/station identifier/i);
    fireEvent.change(field, { target: { value: 'EOC-1' } });
    expect((field as HTMLInputElement).value).toBe('EOC-1');
    expect(screen.getByTestId('probe-identifier')).toHaveTextContent('EOC-1');
  });

  // ── Submit behaviour ────────────────────────────────────────────────────

  it('allows blank submit (identifier optional) — routes to the Location step', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('location');
    });
  });

  it('submits with the identifier and passes it to invoke (grid set later in Location)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.change(screen.getByLabelText(/station identifier/i), { target: { value: 'ARES-NET' } });
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_persist_offline', expect.objectContaining({
        identifier: 'ARES-NET',
        grid: '',
      }));
    });
  });

  it('routes to the Location step on successful submit', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('location');
    });
  });

  it('disables the submit button while inFlight', async () => {
    // Pause invoke so inFlight stays true
    let resolveInvoke: () => void;
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      () => new Promise<void>((resolve) => { resolveInvoke = resolve; })
    );
    render(<WizardProvider><StepProbe /></WizardProvider>);
    const btn = screen.getByRole('button', { name: /continue offline/i });
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);
    // After click inFlight=true → button disabled
    await waitFor(() => expect(btn).toBeDisabled());
    // Resolve the in-flight call
    await act(async () => { resolveInvoke(); });
    await waitFor(() => expect(btn).not.toBeDisabled());
  });

  it('shows an error banner on invoke failure, does NOT route away', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({
      kind: 'ConfigWrite',
      detail: { detail: 'disk full' },
    });
    render(<WizardProvider><StepProbe /></WizardProvider>);
    const initialStep = screen.getByTestId('probe-step').textContent;
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
    // Step must NOT have advanced past offline_identity
    expect(screen.getByTestId('probe-step')).toHaveTextContent(initialStep!);
    expect(screen.getByTestId('probe-step')).not.toHaveTextContent('location');
  });

  it('Busy error shows no user-visible message (silent per spec §3.5)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({ kind: 'Busy' });
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    // Wait a tick to let the promise settle
    await waitFor(() => {
      // No alert rendered for Busy
      const alerts = screen.queryAllByRole('alert');
      const nonEmpty = alerts.filter(a => a.textContent && a.textContent.trim() !== '');
      expect(nonEmpty).toHaveLength(0);
    });
  });
});
