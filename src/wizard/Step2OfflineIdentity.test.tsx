// Step2OfflineIdentity.test.tsx — Phase 4 Task 4.1
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.3 (Step 2-offline) + §5.4
// Plan: Phase 4
//
// Tests: blank-submit allowed, identifier accepts tactical strings,
//        grid accepts 4-char, SUBMIT_OFFLINE_SUCCESS routes to complete,
//        Busy returned silently (no error shown), inFlight disables submit.

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
      <div data-testid="probe-grid">{state.grid}</div>
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

  it('renders identifier field (optional), grid field (optional), and a continue button', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    expect(screen.getByLabelText(/station identifier/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/grid locator/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /continue offline/i })).toBeInTheDocument();
  });

  it('renders the "all fields optional" footer copy', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    expect(screen.getByText(/all fields optional/i)).toBeInTheDocument();
  });

  // ── Field behaviour ─────────────────────────────────────────────────────

  it('identifier field accepts a free-form tactical string', () => {
    render(<WizardProvider><StepProbe /></WizardProvider>);
    const field = screen.getByLabelText(/station identifier/i);
    fireEvent.change(field, { target: { value: 'EOC-1' } });
    expect((field as HTMLInputElement).value).toBe('EOC-1');
    expect(screen.getByTestId('probe-identifier')).toHaveTextContent('EOC-1');
  });

  it('grid field accepts a 4-char Maidenhead locator', () => {
    render(<WizardProvider><StepProbe /></WizardProvider>);
    const field = screen.getByLabelText(/grid locator/i);
    fireEvent.change(field, { target: { value: 'EM75' } });
    expect((field as HTMLInputElement).value).toBe('EM75');
    expect(screen.getByTestId('probe-grid')).toHaveTextContent('EM75');
  });

  it('grid field shows a validation error for invalid grid (non-empty)', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    const field = screen.getByLabelText(/grid locator/i);
    fireEvent.change(field, { target: { value: 'ZZZZ' } });
    fireEvent.blur(field);
    expect(screen.getByRole('alert')).toBeInTheDocument();
  });

  it('grid field does NOT show a validation error when empty', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    const field = screen.getByLabelText(/grid locator/i);
    fireEvent.blur(field);  // blur with empty — no error
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  // ── Submit behaviour ────────────────────────────────────────────────────

  it('allows blank submit (all fields optional) — routes to complete', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    });
  });

  it('submits with identifier and grid, passes them to invoke', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.change(screen.getByLabelText(/station identifier/i), { target: { value: 'ARES-NET' } });
    fireEvent.change(screen.getByLabelText(/grid locator/i), { target: { value: 'EM75' } });
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_persist_offline', expect.objectContaining({
        identifier: 'ARES-NET',
        grid: 'EM75',
      }));
    });
  });

  it('routes to complete on successful submit', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    render(<WizardProvider><StepProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /continue offline/i }));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
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
    // Step must NOT have advanced to 'complete'
    expect(screen.getByTestId('probe-step')).toHaveTextContent(initialStep!);
    expect(screen.getByTestId('probe-step')).not.toHaveTextContent('complete');
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

  it('invalid grid blocks submit — button disabled', () => {
    render(<WizardProvider><Step2OfflineIdentity /></WizardProvider>);
    const field = screen.getByLabelText(/grid locator/i);
    fireEvent.change(field, { target: { value: 'NOTGRID' } });
    const btn = screen.getByRole('button', { name: /continue offline/i });
    expect(btn).toBeDisabled();
  });
});
