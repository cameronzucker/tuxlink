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
    // Default: route by command name.
    //  - wizard_run_test_send → mocked Success outcome
    //  - wizard_test_send_is_mocked → false (live mode) unless a test overrides it
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.resolve({
        kind: 'Success',
        detail: { reply_subject: 'Re: test [MOCKED]' },
      });
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

  // ── MOCKED banner (tuxlink-fzm) ─────────────────────────────────────────

  it('queries wizard_test_send_is_mocked on mount (not gated on sending)', async () => {
    // tuxlink-fzm: the mock signal is a static process env var, so it must be
    // queried on mount — resolved before any send — so a fast mocked send
    // cannot return before the banner signal is known. The prior code queried
    // only on entering `sending`, racing a fast mock return.
    renderInState(); // idle
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_test_send_is_mocked');
    });
  });

  it('MOCKED banner is visible in sending when the backend reports mocked', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(true);
      return Promise.resolve({ kind: 'Success', detail: { reply_subject: 'Re: [MOCKED]' } });
    });
    renderInState({ testSendSubstate: 'sending' });
    await waitFor(() => {
      expect(screen.getByTestId('mock-banner')).toBeInTheDocument();
    });
  });

  it('MOCKED banner is absent in sending when live (is_mocked false)', async () => {
    // Default beforeEach mock returns is_mocked → false.
    renderInState({ testSendSubstate: 'sending' });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('wizard_test_send_is_mocked');
    });
    expect(screen.queryByTestId('mock-banner')).not.toBeInTheDocument();
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
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.resolve({ kind: 'Success', detail: { reply_subject: 'Re: test' } });
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
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.resolve({ kind: 'Failed', detail: { cause: 'connection refused', likely_causes_hint: [] } });
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

  // FIX 2 (P0b): a Busy (mutex-contended) result is a NO-OP — it MUST NOT flip the
  // UI to `failed` while a prior live send may still be in flight. The substate
  // stays whatever it was (here: `sending`, after BEGIN_TEST_SEND).
  it('invoke rejecting with WizardError.Busy is a NO-OP (stays in sending; does NOT go to failed)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.reject({ kind: 'Busy', detail: {} });
    });

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });

    // Allow the rejected invoke promise to settle.
    await act(async () => { await Promise.resolve(); });

    // Busy is swallowed: we entered `sending` via BEGIN_TEST_SEND and stay there.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');
    expect(screen.queryByTestId('substate-failed')).not.toBeInTheDocument();
  });

  it('invoke throwing WizardError.Other transitions to failed substate', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.reject({ kind: 'Other', detail: { detail: 'network unreachable' } });
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

  // ── FIX 1 (P0a): Retry routes THROUGH the reducer (failed → sending) ──────
  // The retry gesture must leave `failed` and enter `sending` BEFORE/at the invoke,
  // so the Retry button (only rendered in `failed`) is gone the instant a send is
  // in flight. This closes the double-transmission-under-one-retry-gesture window.

  it('failed: [Retry] transitions through the reducer to sending (Retry control gone once sending)', async () => {
    // Hold the invoke pending so we can observe the intermediate `sending` substate
    // before the result resolves.
    let resolveSend: (v: unknown) => void = () => {};
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return new Promise((res) => { resolveSend = res; });
    });

    renderInState({ testSendSubstate: 'failed', testSendError: 'connection refused' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('retry-btn'));
    });

    // We are now in `sending` (reducer-routed), and the Retry/Send activation
    // surface is UNCONDITIONALLY ABSENT — no second transmission can be triggered.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');
    expect(screen.queryByTestId('retry-btn')).not.toBeInTheDocument();
    expect(screen.queryByTestId('send-test-btn')).not.toBeInTheDocument();

    // Resolve the in-flight send; UI advances to success.
    await act(async () => {
      resolveSend({ kind: 'Success', detail: { reply_subject: 'Re: test' } });
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('success');
    });
  });

  it('failed: [Retry] then a Busy result is a NO-OP (stays in sending; does NOT bounce to failed)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return Promise.reject({ kind: 'Busy', detail: {} });
    });

    renderInState({ testSendSubstate: 'failed', testSendError: 'connection refused' });
    await act(async () => {
      fireEvent.click(screen.getByTestId('retry-btn'));
    });
    await act(async () => { await Promise.resolve(); });

    // Retry routed us to `sending`; the Busy rejection is swallowed → stay in `sending`.
    expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');
  });

  // ── FIX 4 (P2): MOCKED banner reachable during `sending` when mock mode active ──
  // Spec §3.8 line 348: the MOCKED banner MUST render during `sending` so mock mode
  // is operator-visible. The component queries wizard_test_send_is_mocked on entering
  // `sending` and seeds a [MOCKED] line into testSendLog, which drives the banner.

  it('mock mode: MOCKED banner renders during sending when wizard_test_send_is_mocked=true', async () => {
    // Keep the send pending so we observe `sending` with the banner.
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(true);
      return new Promise(() => { /* never resolves: stay in sending */ });
    });

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });
    await act(async () => { await Promise.resolve(); });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');
      expect(screen.getByTestId('mock-banner')).toBeInTheDocument();
      expect(screen.getByTestId('mock-banner').textContent).toMatch(/MOCKED/i);
    });
  });

  it('live mode: MOCKED banner is ABSENT during sending when wizard_test_send_is_mocked=false', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
      return new Promise(() => { /* never resolves */ });
    });

    renderInState();
    await act(async () => {
      fireEvent.click(screen.getByTestId('send-test-btn'));
    });
    await act(async () => { await Promise.resolve(); });

    await waitFor(() => {
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');
    });
    expect(screen.queryByTestId('mock-banner')).not.toBeInTheDocument();
  });

  // ── tuxlink-9w8: sending-substate watchdog ────────────────────────────
  // Scenario: a Busy result from a *different* holder means TEST_SEND_RESULT
  // never arrives. The watchdog must fire and transition to a recoverable `failed`
  // state so the operator is never stuck in `sending` indefinitely.
  //
  // Watchdog timeout: 45 000 ms — generous enough to never false-fire on a
  // legitimately slow live send (backend polls up to 30 s per wizard.rs
  // TEST_SEND_TIMEOUT_SECS). The watchdog is only the backstop for the
  // genuinely-stuck case.

  describe('sending-substate watchdog (tuxlink-9w8)', () => {
    // Use fake timers for the whole watchdog block so we can advance time without
    // real 45-second waits.
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('watchdog: no TEST_SEND_RESULT within 45 s → transitions to recoverable failed state', async () => {
      // Keep the invoke permanently pending (simulates a Busy from a different
      // holder — the real wizard_run_test_send call that holds the mutex never
      // delivers a TEST_SEND_RESULT to THIS window).
      (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
        if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
        return new Promise(() => { /* never resolves — simulates stuck-in-sending */ });
      });

      renderInState({ testSendSubstate: 'sending' });

      // Before the watchdog fires, we are still in `sending`.
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('sending');

      // Advance fake clock past the 45-second watchdog.
      await act(async () => {
        vi.advanceTimersByTime(45_001);
      });

      // Watchdog fired: must be in `failed` with a clear "timed out / busy" message.
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('failed');
      // The error copy must mention timeout so the operator knows it is recoverable.
      expect(screen.getByTestId('failed-error')).toBeInTheDocument();
      expect(screen.getByTestId('failed-error').textContent).toMatch(/timed out|busy/i);
      // Operator must not be stuck — the [Retry] control must be present.
      expect(screen.getByTestId('retry-btn')).toBeInTheDocument();
    });

    it('watchdog: TEST_SEND_RESULT arrives before 45 s → watchdog cancelled (no spurious timeout-failure)', async () => {
      // The invoke resolves quickly — legitimate fast send.
      (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
        if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
        return Promise.resolve({ kind: 'Success', detail: { reply_subject: 'Re: test' } });
      });

      renderInState();
      await act(async () => {
        fireEvent.click(screen.getByTestId('send-test-btn'));
      });
      await act(async () => { await Promise.resolve(); });

      // Already in `success` — the watchdog should have been cancelled.
      expect(screen.getByTestId('probe-substate')).toHaveTextContent('success');

      // Advance way past the watchdog window — if the timer was NOT cancelled this
      // would fire and transition us back to `failed`, which we must not see.
      await act(async () => {
        vi.advanceTimersByTime(90_000);
      });

      // Still in `success` (or `complete` if auto-advance also ran) — NOT `failed`.
      const substate = screen.getByTestId('probe-substate').textContent;
      expect(substate).not.toBe('failed');
    });

    it('watchdog: leaving `sending` via Skip cancels the watchdog (no late transition after unmount / substate change)', async () => {
      // Keep the invoke permanently pending.
      (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
        if (cmd === 'wizard_test_send_is_mocked') return Promise.resolve(false);
        return new Promise(() => { /* never resolves */ });
      });

      renderInState({ testSendSubstate: 'sending' });

      // Operator skips while the watchdog is running — leaves `sending`.
      await act(async () => {
        fireEvent.click(screen.getByTestId('skip-and-go-btn'));
      });

      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');

      // Now advance past the watchdog window — if not cancelled, the watchdog
      // would fire a late dispatch (possibly a setState-after-unmount React warning,
      // and certainly a wrong `failed` transition).
      await act(async () => {
        vi.advanceTimersByTime(90_000);
      });

      // Must still be `complete` (step), NOT `failed`.
      expect(screen.getByTestId('probe-step')).toHaveTextContent('complete');
    });
  });
});
