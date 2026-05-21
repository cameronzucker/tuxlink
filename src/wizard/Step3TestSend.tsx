// Step3TestSend.tsx — wizard cluster Task 11 / tuxlink-e4x
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//       §3.3 (Step 3), §3.4 (4-substate machine), §5.3 (UX copy), §5.8 (Part 97 dedup)
//
// Part 97 / RADIO-1 safety:
//   The [Send test] button is UNCONDITIONALLY ABSENT when testSendSubstate !== 'idle'.
//   This is NOT just "disabled" — the button is removed from the DOM entirely so
//   a React double-render (StrictMode, Suspense flush) cannot dispatch BEGIN_TEST_SEND
//   twice and trigger two CMS transmissions. See spec §3.1 invariant 2 + §5.8.
//
//   The reducer's BEGIN_TEST_SEND guard (wizardReducer.ts) is the first defense.
//   The Rust-side WizardMutex is the second defense.
//   The button-absent rule is the third (removes the dispatch surface entirely).
//
// 4 substates:
//   idle    → "Ready to send a test message..." + [Send test] [Skip]
//   sending → progress indicator + session-log stream + [Skip and go to inbox]
//   success → green check + auto-advance to complete after 3s
//   failed  → yellow warning + likely-causes list + [Retry] [Edit credentials] [Go to inbox]
//
// Non-blocking: every substate has a path to the inbox (Skip / Go to inbox).
// Transport-visibility paragraph always rendered above the substate content
// per spec §5.3 + UX anti-pattern fix in design doc §4.1.
//
// MOCKED mode: when TUXLINK_TEST_SEND_MOCK is set in the Rust environment,
// wizard_run_test_send returns mocked outcomes. The UI shows a banner:
//   "Test-send MOCKED — no real Winlink transmission."
// The mock signal is sourced from the backend via wizard_test_send_is_mocked(),
// queried on entering the `sending` substate so the banner is reliably visible
// during `sending` (spec §3.8). The prior reply_subject-sniffing approach was
// unreachable because reply_subject is never appended to testSendLog.

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useWizard } from './wizardContext';
import type { TestSendOutcome, WizardError } from './types';

// How long to linger on the success substate before auto-advancing (ms).
// Spec §3.4: "auto-advance to complete after 3 seconds (cancellable)."
const SUCCESS_AUTO_ADVANCE_MS = 3000;

// Watchdog timeout for the `sending` substate (tuxlink-9w8).
//
// The backend live test-send polls up to TEST_SEND_TIMEOUT_SECS (30 s per
// wizard.rs). A GENEROUS margin (15 s) is added so the watchdog never
// false-fires on a legitimately slow but working send. The watchdog is
// ONLY a backstop for the stuck case — a Busy result from a *different*
// mutex holder that means TEST_SEND_RESULT never arrives for this window.
//
// 45 000 ms = 30 s (backend limit) + 15 s (front-end margin)
const SENDING_WATCHDOG_MS = 45_000;

// Narrow an unknown caught value to a discriminated WizardError-by-`kind`.
// Tauri rejects the invoke promise with the serialized WizardError object.
function errorKind(err: unknown): WizardError['kind'] | null {
  if (typeof err === 'object' && err !== null && 'kind' in err) {
    return (err as { kind: WizardError['kind'] }).kind;
  }
  return null;
}

export function Step3TestSend() {
  const { state, dispatch } = useWizard();
  const autoAdvanceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Watchdog timer ref for the `sending` substate (tuxlink-9w8).
  // Armed when `sending` is entered; cancelled when a result arrives or the
  // substate leaves `sending` (including unmount cleanup). If the timer fires,
  // the wizard transitions to a recoverable `failed` state so the operator is
  // never stuck indefinitely.
  const sendingWatchdogTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // FIX 4 (P2): MOCKED-mode signal sourced from the Rust backend (the only
  // authority on whether TUXLINK_TEST_SEND_MOCK is set). Queried once we enter
  // `sending` so the MOCKED banner is operator-visible during that substate
  // (spec §3.8). Null = not yet known; false = live; true = mocked.
  const [isMocked, setIsMocked] = useState<boolean | null>(null);

  // ── Auto-advance on success ────────────────────────────────────────────
  // Schedule auto-advance to 'complete' 3s after entering 'success' substate.
  // The timer is cancelled if the component unmounts or the substate changes
  // (e.g., operator clicks away, or wizard re-mounts for some reason).
  useEffect(() => {
    if (state.testSendSubstate === 'success') {
      autoAdvanceTimer.current = setTimeout(() => {
        dispatch({ type: 'SKIP_TEST_SEND' });
      }, SUCCESS_AUTO_ADVANCE_MS);
    }
    return () => {
      if (autoAdvanceTimer.current !== null) {
        clearTimeout(autoAdvanceTimer.current);
        autoAdvanceTimer.current = null;
      }
    };
  }, [state.testSendSubstate, dispatch]);

  // ── Sending-substate watchdog (tuxlink-9w8) ────────────────────────────
  // Problem: a Busy result from a *different* mutex holder causes the Busy
  // no-op path to stay in `sending` with no TEST_SEND_RESULT ever arriving,
  // stranding the wizard indefinitely.
  //
  // Fix: arm a watchdog when `sending` is entered. If no result arrives within
  // SENDING_WATCHDOG_MS, transition to a recoverable `failed` state. Cancel the
  // watchdog whenever:
  //   • A TEST_SEND_RESULT arrives (real success/failure — substate leaves `sending`)
  //   • The operator skips (SKIP_TEST_SEND dispatched, substate leaves `sending`)
  //   • The component unmounts (effect cleanup)
  //
  // Concurrent-retry case: a Busy from a CONCURRENT test-send double-fire means
  // the OTHER in-flight send will eventually deliver a TEST_SEND_RESULT that
  // changes the substate, which cancels the watchdog before it fires. The
  // generous 45 s window ensures the watchdog never false-fires on a slow
  // legitimate send.
  useEffect(() => {
    if (state.testSendSubstate === 'sending') {
      sendingWatchdogTimer.current = setTimeout(() => {
        sendingWatchdogTimer.current = null;
        dispatch({
          type: 'TEST_SEND_RESULT',
          outcome: {
            kind: 'Failed',
            detail: {
              cause: 'Send timed out — the system was busy; try again.',
              likely_causes_hint: [
                'Another wizard window may be running a test send',
                'No internet connection',
                'Firewall blocking port 8773',
                'CMS temporarily busy',
              ],
            },
          },
        });
      }, SENDING_WATCHDOG_MS);
    } else {
      // Substate left `sending` (result arrived, skip, or error). Cancel the
      // watchdog so it does not fire late.
      if (sendingWatchdogTimer.current !== null) {
        clearTimeout(sendingWatchdogTimer.current);
        sendingWatchdogTimer.current = null;
      }
    }
    return () => {
      // Unmount cleanup: cancel the watchdog so it cannot dispatch after unmount.
      if (sendingWatchdogTimer.current !== null) {
        clearTimeout(sendingWatchdogTimer.current);
        sendingWatchdogTimer.current = null;
      }
    };
  }, [state.testSendSubstate, dispatch]);

  // ── MOCKED-mode detection ───────────────────────────────────────────────
  // FIX 4 (P2) + tuxlink-fzm: ask the Rust backend whether the test-send is
  // mocked, so the MOCKED banner can render during `sending` (spec §3.8).
  //
  // Queried ONCE on mount (not on entering `sending`): the mock signal
  // (TUXLINK_TEST_SEND_MOCK) is a STATIC process env var that cannot change
  // during the wizard, so resolving it at mount means `isMocked` is known well
  // before any send. The prior per-`sending` query raced a fast mocked send —
  // the send could return (leaving `sending`, cancelling the effect) before the
  // query resolved, so the banner never appeared. No idle-reset is needed
  // (the signal is immutable). A query failure → live (banner absent — fail
  // safe: never falsely claim a live send is mocked; Part 97).
  useEffect(() => {
    let cancelled = false;
    invoke<boolean>('wizard_test_send_is_mocked')
      .then((mocked) => {
        if (!cancelled) setIsMocked(mocked);
      })
      .catch(() => {
        if (!cancelled) setIsMocked(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Send handler ───────────────────────────────────────────────────────
  // Part 97: only reachable when testSendSubstate === 'idle' (button absent otherwise).
  const handleSendTest = useCallback(async () => {
    // Guard: no-op if not idle (defense-in-depth; reducer is the primary guard).
    if (state.testSendSubstate !== 'idle') return;

    // Dispatch BEGIN_TEST_SEND to enter 'sending' substate.
    // The reducer's BEGIN_TEST_SEND guard ensures this is a no-op if already sending.
    dispatch({ type: 'BEGIN_TEST_SEND' });

    try {
      const outcome = await invoke<TestSendOutcome>('wizard_run_test_send');
      dispatch({ type: 'TEST_SEND_RESULT', outcome });
    } catch (err) {
      // FIX 2 (P0b): a Busy result means the Rust single-flight mutex was
      // contended — a prior wizard_run_test_send is STILL in flight (and may
      // already be transmitting under the operator's callsign). This contended
      // call did NOT transmit (the mutex returns Busy before run_test_send_impl).
      // Treat it as a strict NO-OP: do NOT dispatch TEST_SEND_RESULT, do NOT flip
      // the UI to `failed`. Flipping to `failed` would put a live Retry control on
      // screen while the first send is still running — a double-transmission risk.
      // We stay in the current (`sending`) substate, reflecting reality.
      if (errorKind(err) === 'Busy') return;

      // tuxlink-2a7: expected operational failures (CMS timeout, connect
      // failure, etc.) now arrive as Ok(TestSendOutcome::Failed) and are
      // dispatched above — they no longer reach this catch. This branch is the
      // fallback for an UNEXPECTED command-level error (IPC failure, panic):
      // surface it as a failed outcome so the user is never stuck in `sending`.
      const detail =
        typeof err === 'object' && err !== null && 'detail' in err
          ? String((err as { detail: unknown }).detail)
          : String(err);
      dispatch({
        type: 'TEST_SEND_RESULT',
        outcome: {
          kind: 'Failed',
          detail: {
            cause: `Unexpected error: ${detail}`,
            likely_causes_hint: [
              'No internet connection',
              'Firewall blocking port 8773',
              'CMS temporarily busy',
              'A captive portal / network login page intercepting traffic',
            ],
          },
        },
      });
    }
  }, [state.testSendSubstate, dispatch]);

  // ── Skip handler ───────────────────────────────────────────────────────
  const handleSkip = useCallback(() => {
    dispatch({ type: 'SKIP_TEST_SEND' });
  }, [dispatch]);

  // ── Retry handler ─────────────────────────────────────────────────────
  // FIX 1 (P0a): the [Retry] gesture is routed THROUGH the reducer.
  //
  // RETRY_TEST_SEND transitions failed → sending in the reducer, so React leaves
  // `failed` and enters `sending` at the moment of invoke. The [Retry] button is
  // rendered ONLY in the `failed` substate, so once we are in `sending` there is no
  // activation surface for a second transmission — preserving the Part 97
  // one-consent-one-transmission invariant (spec §3.1 invariant 2 + §5.8).
  //
  // Previously this handler bypassed the reducer and called invoke() while the
  // substate stayed `failed` (Retry button still live), so a fast completion could
  // release the Rust mutex and a second Retry could transmit again under one gesture.
  const handleRetry = useCallback(async () => {
    if (state.testSendSubstate !== 'failed') return;

    // Leave `failed`, enter `sending` BEFORE the invoke. The reducer no-ops this
    // from any non-`failed` substate, so a double-fire from `sending` is harmless.
    dispatch({ type: 'RETRY_TEST_SEND' });

    try {
      const outcome = await invoke<TestSendOutcome>('wizard_run_test_send');
      dispatch({ type: 'TEST_SEND_RESULT', outcome });
    } catch (err) {
      // FIX 2 (P0b): Busy means the single-flight mutex was contended — a prior
      // send is still in flight. The contended call did NOT transmit. No-op: do
      // not surface a failure (which would re-show the live Retry control mid-flight).
      if (errorKind(err) === 'Busy') return;

      const detail =
        typeof err === 'object' && err !== null && 'detail' in err
          ? String((err as { detail: unknown }).detail)
          : String(err);
      dispatch({
        type: 'TEST_SEND_RESULT',
        outcome: {
          kind: 'Failed',
          detail: {
            cause: `Unexpected error on retry: ${detail}`,
            likely_causes_hint: [
              'No internet connection',
              'Firewall blocking port 8773',
              'CMS temporarily busy',
              'A captive portal / network login page intercepting traffic',
            ],
          },
        },
      });
    }
  }, [state.testSendSubstate, dispatch]);

  // ── Edit credentials handler ──────────────────────────────────────────
  const handleEditCredentials = useCallback(() => {
    dispatch({ type: 'RETURN_TO_CREDENTIALS' });
  }, [dispatch]);

  // ── Mock banner ───────────────────────────────────────────────────────
  // FIX 4 (P2): the banner is driven by the backend mock signal (isMocked),
  // queried on entering `sending`. This makes the banner reliably reachable
  // during the `sending` substate (spec §3.8 line 348) — the prior approach
  // sniffed testSendLog for "[MOCKED]", but reply_subject was never appended
  // to testSendLog, so the banner was effectively unreachable.
  // Rendered in sending/success/failed (any in-flight or terminal substate);
  // absent in live mode and in idle.
  const showMockBanner = isMocked === true && state.testSendSubstate !== 'idle';
  const mockBanner = showMockBanner ? (
    <p className="wizard-mock-banner" role="note" data-testid="mock-banner">
      Test-send MOCKED — no real Winlink transmission.
    </p>
  ) : null;

  // ── Render ────────────────────────────────────────────────────────────
  return (
    <div className="wizard-step wizard-step-test-send">
      {/* Transport-visibility paragraph — always rendered per spec §5.3 + design doc §4.1 */}
      <p className="wizard-transport-visibility" data-testid="transport-visibility">
        Tuxlink uses CMS-SSL (TLS-encrypted, port 8773) by default. You can change
        this in <strong>Settings → Connection</strong> if your network doesn't allow
        port 8773.
      </p>

      {/* ── idle substate ─────────────────────────────────────────────── */}
      {state.testSendSubstate === 'idle' && (
        <div data-testid="substate-idle">
          <h1>Verify your CMS credentials</h1>
          <p>
            Ready to send a test message to verify your credentials. This sends a
            brief message to <code>SERVICE@winlink.org</code> and waits for an
            autoresponder reply.
          </p>
          {/* Part 97 / RADIO-1: [Send test] button is PRESENT only in idle substate.
              It is ABSENT (not rendered) in sending/success/failed — see spec §3.1 invariant 2 + §5.8. */}
          <div className="wizard-submit-row">
            <button
              type="button"
              data-testid="send-test-btn"
              onClick={handleSendTest}
            >
              Send test
            </button>
            <button
              type="button"
              data-testid="skip-btn"
              onClick={handleSkip}
              className="wizard-btn-secondary"
            >
              Skip
            </button>
          </div>
        </div>
      )}

      {/* ── sending substate ──────────────────────────────────────────── */}
      {state.testSendSubstate === 'sending' && (
        <div data-testid="substate-sending">
          <h1>Sending test message…</h1>
          {/* MOCKED banner — operator-visible during `sending` per spec §3.8 (FIX 4). */}
          {mockBanner}
          {/* Session-log preview — human-shaped projection lines streamed via Tauri events */}
          <div
            className="wizard-session-log"
            role="log"
            aria-live="polite"
            data-testid="session-log"
          >
            {state.testSendLog.length === 0 ? (
              <p className="wizard-log-placeholder">Connecting to CMS via TLS (port 8773)…</p>
            ) : (
              state.testSendLog.map((line, i) => (
                // eslint-disable-next-line react/no-array-index-key
                <p key={i} className="wizard-log-line">{line}</p>
              ))
            )}
          </div>
          {/* [Skip and go to inbox] — always available during sending per spec §3.4 */}
          <div className="wizard-submit-row">
            <button
              type="button"
              data-testid="skip-and-go-btn"
              onClick={handleSkip}
              className="wizard-btn-secondary"
            >
              Skip and go to inbox
            </button>
          </div>
          {/* Note: [Send test] button is ABSENT here — spec §3.1 invariant 2 + §5.8 Part 97 */}
        </div>
      )}

      {/* ── success substate ──────────────────────────────────────────── */}
      {state.testSendSubstate === 'success' && (
        <div data-testid="substate-success">
          <span
            className="wizard-success-icon"
            role="img"
            aria-label="Success"
            data-testid="success-icon"
          >
            ✓
          </span>
          <h1>Test send complete.</h1>
          <p data-testid="success-message">Your CMS account is verified.</p>
          {mockBanner}
          <p className="wizard-auto-advance-hint">
            Continuing to inbox in 3 seconds…{' '}
            <button
              type="button"
              onClick={() => {
                if (autoAdvanceTimer.current !== null) {
                  clearTimeout(autoAdvanceTimer.current);
                  autoAdvanceTimer.current = null;
                }
                dispatch({ type: 'SKIP_TEST_SEND' });
              }}
              data-testid="go-to-inbox-now-btn"
              className="wizard-btn-link"
            >
              Go to inbox now
            </button>
          </p>
          {/* Note: [Send test] button ABSENT — spec §3.1 invariant 2 + §5.8 Part 97 */}
        </div>
      )}

      {/* ── failed substate ───────────────────────────────────────────── */}
      {state.testSendSubstate === 'failed' && (
        <div data-testid="substate-failed">
          <span
            className="wizard-warning-icon"
            role="img"
            aria-label="Warning"
            data-testid="warning-icon"
          >
            ⚠
          </span>
          <h1>Test send did not complete.</h1>
          {/* Yellow warning copy — NOT red error (spec §3.4: "failure is information, not a wall") */}
          <p className="wizard-failed-warning" data-testid="failed-message">
            Your credentials are saved. You can reach your inbox now and retry the
            test send later from <strong>Session → Test send</strong>.
          </p>
          {/* Specific error from testSendError */}
          {state.testSendError && (
            <p
              className="wizard-failed-detail"
              role="alert"
              data-testid="failed-error"
            >
              Error: {state.testSendError}
            </p>
          )}
          {/* Likely-causes list per spec §3.4 + §5.12 (captive portal as 4th cause) */}
          <p className="wizard-failed-hint-heading">Likely causes:</p>
          <ul className="wizard-failed-causes" data-testid="likely-causes">
            <li>No internet connection</li>
            <li>Firewall blocking port 8773</li>
            <li>CMS temporarily busy</li>
            <li>A captive portal or network login page intercepting traffic</li>
          </ul>
          <div className="wizard-submit-row">
            {/* [Retry] — re-invokes wizard_run_test_send; see handleRetry above */}
            <button
              type="button"
              data-testid="retry-btn"
              onClick={handleRetry}
            >
              Retry
            </button>
            {/* [Edit credentials] — dispatches RETURN_TO_CREDENTIALS (wrong-password recovery) */}
            <button
              type="button"
              data-testid="edit-credentials-btn"
              onClick={handleEditCredentials}
              className="wizard-btn-secondary"
            >
              Edit credentials
            </button>
            {/* [Go to inbox] — dispatches SKIP_TEST_SEND */}
            <button
              type="button"
              data-testid="go-to-inbox-btn"
              onClick={handleSkip}
              className="wizard-btn-secondary"
            >
              Go to inbox
            </button>
            {/* [Open Settings] — v0.0.1: rendered disabled with tooltip per spec §3.4 */}
            <button
              type="button"
              data-testid="open-settings-btn"
              disabled
              title="Settings UI lands in a later release"
              className="wizard-btn-secondary"
            >
              Open Settings
            </button>
          </div>
          {/* Note: [Send test] button ABSENT — spec §3.1 invariant 2 + §5.8 Part 97 */}
        </div>
      )}
    </div>
  );
}
