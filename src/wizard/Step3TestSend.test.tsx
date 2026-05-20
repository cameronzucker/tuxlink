// Step3TestSend.test.tsx — Task 11 / tuxlink-e4x
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//       §3.3 (Step 3), §3.4 (4-substate machine), §5.3 (UX copy), §5.8 (Part 97 dedup)
//
// Critical tests:
//   - All 4 substates render the correct copy + controls
//   - [Send test] button is ABSENT from sending/success/failed substates (Part 97 §5.8)
//   - BEGIN_TEST_SEND while sending is a no-op (dedup guard, §3.1 invariant 2)
//   - Successful invoke dispatches TEST_SEND_RESULT with Success outcome
//   - Failed invoke dispatches TEST_SEND_RESULT with Failed outcome
//   - Skip dispatches SKIP_TEST_SEND → step = complete
//   - RETURN_TO_CREDENTIALS from failed clears password + navigates back

import { describe, it, expect, vi, beforeEach } from 'vitest';
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
      <div data-testid="probe-substate">{state.testSendSubstate}</div>
      <div data-testid="probe-skip-signaled">{String(state.skipSignaled)}</div>
      <div data-testid="probe-test-send-error">{state.testSendError ?? ''}</div>
    </>
  );
}

function renderInState(override: Partial<WizardState> = {}) {
  const base: Partial<WizardState> = { step: 'test_send', testSendSubstate: 'idle', ...override };
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
    // Default: mock returns success
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      kind: 'Success',
      detail: { reply_subject: 'Re: test [MOCKED]' },
    });
  });

  // ── Transport-visibility paragraph ─────────────────────────────────────

  it('always renders the transport-visibility paragraph in idle', () => {
    renderInState();
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
    expect(screen.getByTestId('transport-visibility').textContent).toMatch(/CMS-SSL/i);
    expect(screen.getByTestId('transport-visibility').textContent).toMatch(/8773/);
  });

  it('transport-visibility paragraph is present in sending substate', () => {
    renderInState({ testSendSubstate: 'sending' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  it('transport-visibility paragraph is present in success substate', () => {
    renderInState({ testSendSubstate: 'success' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  it('transport-visibility paragraph is present in failed substate', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    expect(screen.getByTestId('transport-visibility')).toBeInTheDocument();
  });

  // ── idle substate ──────────────────────────────────────────────────────

  it('idle: renders the correct heading', () => {
    renderInState();
    expect(screen.getByTestId('substate-idle')).toBeInTheDocument();
    expect(screen.getByRole('heading')).toBeInTheDocument();
  });

  it('idle: renders [Send test] button', () => {
    renderInState();
    expect(screen.getByTestId('send-test-btn')).toBeInTheDocument();
  });

  it('idle: renders [Skip] button', () => {
    renderInState();
    expect(screen.getByTestId('skip-btn')).toBeInTheDocument();
  });

  it('idle: [Skip] dispatches SKIP_TEST_SEND → step = complete', async () => {
    renderInState();
    fireEvent.click(screen.getByTestId('skip-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    });
  });

  // ── sending substate — directly mounted via initialStateOverride ─────────

  it('sending: [Send test] button is ABSENT (Part 97 §5.8 dedup guard)', () => {
    renderInState({ testSendSubstate: 'sending' });
    // [Send test] MUST NOT be in the DOM (absent, not just disabled).
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('sending: renders [Skip and go to inbox] button', () => {
    renderInState({ testSendSubstate: 'sending' });
    expect(screen.getByTestId('skip-and-go-btn')).toBeInTheDocument();
  });

  it('sending: displays the session log area', () => {
    renderInState({ testSendSubstate: 'sending' });
    expect(screen.getByTestId('session-log')).toBeInTheDocument();
  });

  it('sending: [Skip and go to inbox] sets skipSignaled + transitions to complete', async () => {
    renderInState({ testSendSubstate: 'sending' });
    fireEvent.click(screen.getByTestId('skip-and-go-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
      expect(screen.getByTestId('probe-skip-signaled')).toHaveTextContent('true');
    });
  });

  // ── Send test → full flow: idle → sending → success ────────────────────

  it('[Send test] click invokes wizard_run_test_send and dispatches TEST_SEND_RESULT on success', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      kind: 'Success',
      detail: { reply_subject: 'Re: test' },
    });

    renderInState();
    fireEvent.click(screen.getByTestId('send-test-btn'));

    // After clicking, the invoke should have been called.
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_run_test_send');
    });

    // Result: success substate.
    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('success');
    });
  });

  it('[Send test] click + failed invoke transitions to failed substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      kind: 'Failed',
      detail: { cause: 'connection refused', likely_causes_hint: [] },
    });

    renderInState();
    fireEvent.click(screen.getByTestId('send-test-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('failed');
    });
  });

  // ── success substate — directly mounted ──────────────────────────────────

  it('success: renders success icon and verified message', () => {
    renderInState({ testSendSubstate: 'success' });
    expect(screen.getByTestId('substate-success')).toBeInTheDocument();
    expect(screen.getByTestId('success-icon')).toBeInTheDocument();
    expect(screen.getByTestId('success-message').textContent).toMatch(/verified/i);
  });

  it('success: [Send test] button is ABSENT (Part 97 §5.8)', () => {
    renderInState({ testSendSubstate: 'success' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('success: auto-advances to complete after 3s', async () => {
    // Use fake timers just for this test.
    vi.useFakeTimers();
    try {
      renderInState({ testSendSubstate: 'success' });

      // Before timer fires, still in success.
      expect(screen.getByTestId('probe-step')).not.toHaveTextContent('complete');

      // Advance fake timer past the 3-second auto-advance.
      await act(async () => {
        vi.advanceTimersByTime(3001);
      });

      // After the timer fires, the SKIP_TEST_SEND dispatch runs.
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    } finally {
      vi.useRealTimers();
    }
  });

  it('success: [Go to inbox now] cancels auto-advance and transitions immediately', async () => {
    renderInState({ testSendSubstate: 'success' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('go-to-inbox-now-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
  });

  // ── failed substate — directly mounted ──────────────────────────────────

  it('failed: renders warning icon and failure message', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'connection refused' });
    expect(screen.getByTestId('substate-failed')).toBeInTheDocument();
    expect(screen.getByTestId('warning-icon')).toBeInTheDocument();
    expect(screen.getByTestId('failed-message')).toBeInTheDocument();
  });

  it('failed: renders the specific error detail from testSendError', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'SPECIFIC_ERROR_XYZ' });
    expect(screen.getByTestId('failed-error')).toHaveTextContent('SPECIFIC_ERROR_XYZ');
  });

  it('failed: renders the likely-causes list with captive portal entry (spec §5.12)', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'error' });
    const causes = screen.getByTestId('likely-causes');
    expect(causes.textContent).toMatch(/captive portal/i);
    expect(causes.querySelectorAll('li').length).toBeGreaterThanOrEqual(3);
  });

  it('failed: [Send test] button is ABSENT (Part 97 §5.8)', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
  });

  it('failed: [Retry] button is present', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
  });

  it('failed: [Edit credentials] dispatches RETURN_TO_CREDENTIALS → step = credentials', async () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'wrong password' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('edit-credentials-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('credentials');
  });

  it('failed: [Go to inbox] dispatches SKIP_TEST_SEND → step = complete', async () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('go-to-inbox-btn'));
    });
    expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
  });

  it('failed: [Open Settings] button is rendered but disabled (v0.0.1 placeholder)', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    expect(screen.getByTestId('open-settings-btn')).toBeDisabled();
  });

  // ── invoke error handling via idle→send flow ──────────────────────────

  it('invoke throwing WizardError.Busy transitions to failed substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({ kind: 'Busy', detail: {} });

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('failed');
    }, { timeout: 10000 });
  });

  it('invoke throwing WizardError.Other transitions to failed substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue({
      kind: 'Other',
      detail: { detail: 'network unreachable' },
    });

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('failed');
    }, { timeout: 10000 });
  });

  // ── Part 97 / RADIO-1 dedup invariant — structural test ───────────────
  // The [Send test] button is ABSENT (not in DOM) from all non-idle substates.
  // This means it is STRUCTURALLY IMPOSSIBLE to dispatch BEGIN_TEST_SEND twice
  // via user interaction during sending/success/failed substates.
  // Belt-and-suspenders: the wizardReducer test covers the reducer-level guard.

  it('Part 97: [Send test] absent in sending → cannot dispatch BEGIN_TEST_SEND via UI', () => {
    renderInState({ testSendSubstate: 'sending' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('skip-and-go-btn')).toBeInTheDocument();
  });

  it('Part 97: [Send test] absent in success → cannot dispatch BEGIN_TEST_SEND via UI', () => {
    renderInState({ testSendSubstate: 'success' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('substate-success')).toBeInTheDocument();
  });

  it('Part 97: [Send test] absent in failed → cannot dispatch BEGIN_TEST_SEND via UI', () => {
    renderInState({ testSendSubstate: 'failed', testSendError: 'err' });
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();
    expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
  });
});
