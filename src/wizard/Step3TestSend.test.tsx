// Step3TestSend.test.tsx — Task 5.4 / tuxlink-9phd
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//       §3.3 (Step 3), §3.4 (4-substate machine), §5.3 (UX copy), §5.8 (Part 97 dedup)
//
// Task 5.4 update: replaced Pat-based test-send with connect-only NativeBackend probe.
//   - invoke target: 'wizard_run_test_send' → 'verify_cms_connection'
//   - return type: TestSendOutcome discriminated union → void (null on success) / throws on error
//   - wizard_test_send_is_mocked + MOCKED banner: removed (no mock mode for the probe)
//   - state fields: testSendSubstate → cmsVerifySubstate, testSendError → cmsVerifyError, etc.
//   - action types: BEGIN_TEST_SEND → BEGIN_CMS_VERIFY, TEST_SEND_RESULT → CMS_VERIFY_RESULT, etc.
//   - substates: idle / sending / success / failed → idle / probing / ok / error
//
// Critical tests:
//   - All 4 substates render the correct copy + controls
//   - [Verify CMS Connection] button is ABSENT from probing/ok/error substates (dedup)
//   - BEGIN_CMS_VERIFY while probing is a no-op (dedup guard)
//   - Successful invoke dispatches CMS_VERIFY_RESULT(ok=true)
//   - Failing invoke dispatches CMS_VERIFY_RESULT(ok=false, errorMessage)
//   - Skip dispatches SKIP_CMS_VERIFY → step = complete
//   - RETURN_TO_CREDENTIALS from error clears password + navigates back

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { WizardProvider } from './wizardContext';
import { Step3TestSend } from './Step3TestSend';
import type { WizardState } from './types';
import { useWizard } from './wizardContext';

// ── Tauri mock ─────────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

// ── Helpers ────────────────────────────────────────────────────────────────

function Probe() {
  const { state } = useWizard();
  return (
    <>
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-substate">{state.cmsVerifySubstate}</div>
      <div data-testid="probe-skip-signaled">{String(state.skipSignaled)}</div>
      <div data-testid="probe-cms-verify-error">{state.cmsVerifyError ?? ''}</div>
    </>
  );
}

function renderInState(override: Partial<WizardState> = {}) {
  const base: Partial<WizardState> = { step: 'cms_verify', cmsVerifySubstate: 'idle', ...override };
  render(
    <WizardProvider initialStateOverride={base}>
      <Step3TestSend />
      <Probe />
    </WizardProvider>
  );
}

describe('<Step3TestSend>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Default: verify_cms_connection resolves (success — returns void/null).
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() => Promise.resolve(null));
  });

  // ── Transport-visibility paragraph ─────────────────────────────────────

  it('always renders the transport-visibility paragraph in idle', () => {
    renderInState();
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
    expect(screen.getByTestId('transport-visibility').textContent).toMatch(/CMS-SSL/i);
    expect(screen.getByTestId('transport-visibility').textContent).toMatch(/8773/);
  });

  it('transport-visibility paragraph is present in probing substate', () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  it('transport-visibility paragraph is present in ok substate', () => {
    renderInState({ cmsVerifySubstate: 'ok' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  it('transport-visibility paragraph is present in error substate', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  // ── idle substate ──────────────────────────────────────────────────────

  it('idle: renders the correct heading', () => {
    renderInState();
    expect(screen.getByTestId('substate-idle')).toBeInTheDocument();
    expect(screen.getByRole('heading')).toBeInTheDocument();
  });

  it('idle: renders [Verify CMS Connection] button', () => {
    renderInState();
    expect(screen.getByTestId('send-test-btn')).toBeInTheDocument();
    expect(screen.getByTestId('send-test-btn').textContent).toMatch(/Verify CMS Connection/i);
  });

  it('idle: renders [Skip] button', () => {
    renderInState();
    expect(screen.getByTestId('skip-btn')).toBeInTheDocument();
  });

  it('idle: [Skip] dispatches SKIP_CMS_VERIFY → step = complete', async () => {
    renderInState();
    fireEvent.click(screen.getByTestId('skip-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    });
  });

  // ── probing substate — directly mounted via initialStateOverride ─────────

  it('probing: [Verify CMS Connection] button is ABSENT (dedup guard)', () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('probing: renders [Skip and go to inbox] button', () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    expect(screen.getByTestId('skip-and-go-btn')).toBeInTheDocument();
  });

  it('probing: displays the session log area', () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    expect(screen.getByTestId('session-log')).toBeInTheDocument();
  });

  it('probing: [Skip and go to inbox] sets skipSignaled + transitions to complete', async () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    fireEvent.click(screen.getByTestId('skip-and-go-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
      expect(screen.getByTestId('probe-skip-signaled')).toHaveTextContent('true');
    });
  });

  // ── Verify → full flow: idle → probing → ok ───────────────────────────

  it('[Verify CMS Connection] click invokes verify_cms_connection and dispatches CMS_VERIFY_RESULT ok=true on success', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() => Promise.resolve(null));

    renderInState();
    fireEvent.click(screen.getByTestId('send-test-btn'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('verify_cms_connection');
    });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('ok');
    });
  });

  it('[Verify CMS Connection] click + failing invoke transitions to error substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
      Promise.reject({ kind: 'Other', detail: { detail: 'connection refused' } })
    );

    renderInState();
    fireEvent.click(screen.getByTestId('send-test-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('error');
    });
  });

  // ── ok substate — directly mounted ──────────────────────────────────────

  it('ok: renders success icon and verified message', () => {
    renderInState({ cmsVerifySubstate: 'ok' });
    expect(screen.getByTestId('substate-success')).toBeInTheDocument();
    expect(screen.getByTestId('success-icon')).toBeInTheDocument();
    expect(screen.getByTestId('success-message').textContent).toMatch(/verified/i);
  });

  it('ok: [Verify CMS Connection] button is ABSENT (dedup)', () => {
    renderInState({ cmsVerifySubstate: 'ok' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('ok: auto-advances to complete after 3s', async () => {
    vi.useFakeTimers();
    try {
      renderInState({ cmsVerifySubstate: 'ok' });

      expect(screen.getByTestId('probe-step')).not.toHaveTextContent('complete');

      await act(async () => {
        vi.advanceTimersByTime(3001);
      });

      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    } finally {
      vi.useRealTimers();
    }
  });

  it('ok: [Go to inbox now] cancels auto-advance and transitions immediately', async () => {
    renderInState({ cmsVerifySubstate: 'ok' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('go-to-inbox-now-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
  });

  // ── error substate — directly mounted ──────────────────────────────────

  it('error: renders warning icon and failure message', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'connection refused' });
    expect(screen.getByTestId('substate-failed')).toBeInTheDocument();
    expect(screen.getByTestId('warning-icon')).toBeInTheDocument();
    expect(screen.getByTestId('failed-message')).toBeInTheDocument();
  });

  it('error: renders the specific error detail from cmsVerifyError', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'SPECIFIC_ERROR_XYZ' });
    expect(screen.getByTestId('failed-error')).toHaveTextContent('SPECIFIC_ERROR_XYZ');
  });

  it('error: renders the likely-causes list with captive portal entry (spec §5.12)', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'error' });
    const causes = screen.getByTestId('likely-causes');
    expect(causes.textContent).toMatch(/captive portal/i);
    expect(causes.querySelectorAll('li').length).toBeGreaterThanOrEqual(3);
  });

  it('error: [Verify CMS Connection] button is ABSENT (dedup)', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('error: [Retry] button is present', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
  });

  it('error: [Edit credentials] dispatches RETURN_TO_CREDENTIALS → step = credentials', async () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'wrong password' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('edit-credentials-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('credentials');
  });

  it('error: [Go to inbox] dispatches SKIP_CMS_VERIFY → step = complete', async () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('go-to-inbox-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
  });

  it('error: [Open Settings] button is rendered but disabled (placeholder for now)', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    expect(screen.getByTestId('open-settings-btn')).toBeDisabled();
  });

  // ── invoke error handling via idle→verify flow ────────────────────────

  // Busy (mutex-contended) is a NO-OP — it MUST NOT flip the UI to `error`
  // while a prior probe may still be in flight.
  it('invoke rejecting with WizardError.Busy is a NO-OP (stays in probing; does NOT go to error)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
      Promise.reject({ kind: 'Busy', detail: {} })
    );

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });

    await act(async () => { await Promise.resolve(); });

    // Busy is swallowed: entered `probing` via BEGIN_CMS_VERIFY and stay there.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('probing');
    expect(screen.queryByTestId('substate-failed')).not.toBeInTheDocument();
  });

  it('invoke throwing WizardError.Other transitions to error substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
      Promise.reject({ kind: 'Other', detail: { detail: 'network unreachable' } })
    );

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('error');
    }, { timeout: 10000 });
  });

  // ── Dedup invariant — structural tests ────────────────────────────────
  // The [Verify CMS Connection] button is ABSENT (not in DOM) from all non-idle substates.

  it('Dedup: [Verify CMS Connection] absent in probing → cannot dispatch BEGIN_CMS_VERIFY via UI', () => {
    renderInState({ cmsVerifySubstate: 'probing' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('skip-and-go-btn')).toBeInTheDocument();
  });

  it('Dedup: [Verify CMS Connection] absent in ok → cannot dispatch BEGIN_CMS_VERIFY via UI', () => {
    renderInState({ cmsVerifySubstate: 'ok' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('substate-success')).toBeInTheDocument();
  });

  it('Dedup: [Verify CMS Connection] absent in error → cannot dispatch BEGIN_CMS_VERIFY via UI', () => {
    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'err' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
  });

  // ── Retry routes THROUGH the reducer (error → probing) ──────────────────

  it('error: [Retry] transitions through the reducer to probing (Retry control gone once probing)', async () => {
    let resolveSend: (v: unknown) => void = () => {};
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
      new Promise((res) => { resolveSend = res; })
    );

    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'connection refused' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('retry-btn'));
    });

    // Now in `probing` (reducer-routed); Retry/Verify activation surface is absent.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('probing');
    expect(screen.queryByTestId('retry-btn')).not.toBeInTheDocument();
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();

    // Resolve the in-flight probe; UI advances to ok.
    await act(async () => {
      resolveSend(null);
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('ok');
    });
  });

  it('error: [Retry] then a Busy result is a NO-OP (stays in probing; does NOT bounce to error)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
      Promise.reject({ kind: 'Busy', detail: {} })
    );

    renderInState({ cmsVerifySubstate: 'error', cmsVerifyError: 'connection refused' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('retry-btn'));
    });
    await act(async () => { await Promise.resolve(); });

    // Retry routed us to `probing`; the Busy rejection is swallowed → stay in `probing`.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('probing');
  });

  // ── probing-substate watchdog ─────────────────────────────────────────
  // The watchdog fires if no CMS_VERIFY_RESULT arrives within PROBING_WATCHDOG_MS,
  // transitioning to a recoverable `error` state.

  describe('probing-substate watchdog', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('watchdog: no CMS_VERIFY_RESULT within 90 s → transitions to recoverable error state', async () => {
      (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
        new Promise(() => { /* never resolves — simulates stuck probe */ })
      );

      renderInState({ cmsVerifySubstate: 'probing' });

      expect(screen.getByTestId('probe-substate')).toHaveTextContent('probing');

      await act(async () => {
        vi.advanceTimersByTime(90_001);
      });

      expect(screen.getByTestId('probe-substate')).toHaveTextContent('error');
      expect(screen.getByTestId('failed-error')).toBeInTheDocument();
      expect(screen.getByTestId('failed-error').textContent).toMatch(/timed out|busy/i);
      expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
    });

    it('watchdog: CMS_VERIFY_RESULT arrives before 90 s → watchdog cancelled (no spurious timeout-error)', async () => {
      (invoke as ReturnType<typeof vi.fn>).mockImplementation(() => Promise.resolve(null));

      renderInState();
      await act(async () => {
        fireEvent.click(screen.getByTestId('send-test-btn'));
      });
      await act(async () => { await Promise.resolve(); });

      expect(screen.getByTestId('probe-substate')).toHaveTextContent('ok');

      await act(async () => {
        vi.advanceTimersByTime(180_000);
      });

      const substate = screen.getByTestId('probe-substate').textContent;
      expect(substate).not.toBe('error');
    });

    it('watchdog: leaving `probing` via Skip cancels the watchdog', async () => {
      (invoke as ReturnType<typeof vi.fn>).mockImplementation(() =>
        new Promise(() => { /* never resolves */ })
      );

      renderInState({ cmsVerifySubstate: 'probing' });

      await act(async () => {
        fireEvent.click(screen.getByTestId('skip-and-go-btn'));
      });

      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');

      await act(async () => {
        vi.advanceTimersByTime(180_000);
      });

      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    });
  });
});
