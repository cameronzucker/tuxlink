// Step3TestSend.tsx — wizard cluster Task 5.4 / tuxlink-9phd
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//       §3.3 (Step 3), §3.4 (4-substate machine), §5.3 (UX copy), §5.8 (Part 97 dedup)
//
// Task 5.4 (tuxlink-9phd): replaced the Pat-based test-send with a
// connect-only NativeBackend probe (verify_cms_connection). No transmission;
// just verifies CMS reachability + auth. RADIO-1 no longer applies to this path.
//
// 4 substates:
//   idle    → "Ready to verify..." + [Verify CMS Connection] [Skip]
//   probing → progress indicator + session-log stream + [Skip and go to inbox]
//   ok      → green check + auto-advance to complete after 3s
//   error   → yellow warning + error message + [Retry] [Edit credentials] [Go to inbox]
//
// Non-blocking: every substate has a path to the inbox (Skip / Go to inbox).
// Transport-visibility paragraph always rendered above the substate content
// per spec §5.3 + UX anti-pattern fix in design doc §4.1.
//
// Part 97 dedup (inherited from prior test-send design, still correct here):
//   The [Verify CMS Connection] button is UNCONDITIONALLY ABSENT when
//   cmsVerifySubstate !== 'idle'. This is NOT just "disabled" — the button is
//   removed from the DOM entirely so a React double-render cannot dispatch
//   BEGIN_CMS_VERIFY twice. The reducer's BEGIN_CMS_VERIFY guard (wizardReducer.ts)
//   is the first defense; the Rust-side WizardMutex is the second; the
//   button-absent rule is the third.

import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useWizard } from './wizardContext';
import type { WizardError } from './types';

// How long to linger on the ok substate before auto-advancing (ms).
// Spec §3.4: "auto-advance to complete after 3 seconds (cancellable)."
const OK_AUTO_ADVANCE_MS = 3000;

// Watchdog timeout for the `probing` substate (tuxlink-9w8 pattern).
//
// The backend probe has no built-in timeout (the NativeBackend connect uses OS
// TCP defaults, ~75 s for SYN retries). A GENEROUS watchdog ensures the wizard
// is never stuck indefinitely.
//
// 90 000 ms = 75 s (OS TCP SYN timeout upper bound) + 15 s margin.
const PROBING_WATCHDOG_MS = 90_000;

// Narrow an unknown caught value to a discriminated WizardError-by-`kind`.
// Tauri rejects the invoke promise with the serialized WizardError object.
function errorKind(err: unknown): WizardError['kind'] | null {
  if (typeof err === 'object' && err !== null && 'kind' in err) {
    return (err as { kind: WizardError['kind'] }).kind;
  }
  return null;
}

// Extract a human-readable detail from a caught WizardError or unknown error.
function extractErrorDetail(err: unknown): string {
  if (typeof err === 'object' && err !== null) {
    if ('detail' in err && typeof (err as { detail: unknown }).detail === 'object') {
      const d = (err as { detail: { detail?: unknown } }).detail;
      if (d && 'detail' in d) return String(d.detail);
    }
    if ('detail' in err) return String((err as { detail: unknown }).detail);
  }
  return String(err);
}

export function Step3TestSend() {
  const { state, dispatch } = useWizard();
  const autoAdvanceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Watchdog timer ref for the `probing` substate.
  // Armed when `probing` is entered; cancelled when a result arrives or the
  // substate leaves `probing` (including unmount cleanup).
  const probingWatchdogTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Auto-advance on ok ─────────────────────────────────────────────────
  useEffect(() => {
    if (state.cmsVerifySubstate === 'ok') {
      autoAdvanceTimer.current = setTimeout(() => {
        dispatch({ type: 'SKIP_CMS_VERIFY' });
      }, OK_AUTO_ADVANCE_MS);
    }
    return () => {
      if (autoAdvanceTimer.current !== null) {
        clearTimeout(autoAdvanceTimer.current);
        autoAdvanceTimer.current = null;
      }
    };
  }, [state.cmsVerifySubstate, dispatch]);

  // ── Probing-substate watchdog ──────────────────────────────────────────
  useEffect(() => {
    if (state.cmsVerifySubstate === 'probing') {
      probingWatchdogTimer.current = setTimeout(() => {
        probingWatchdogTimer.current = null;
        dispatch({
          type: 'CMS_VERIFY_RESULT',
          ok: false,
          errorMessage: 'Connection timed out — the CMS did not respond in time. Try again.',
        });
      }, PROBING_WATCHDOG_MS);
    } else {
      if (probingWatchdogTimer.current !== null) {
        clearTimeout(probingWatchdogTimer.current);
        probingWatchdogTimer.current = null;
      }
    }
    return () => {
      if (probingWatchdogTimer.current !== null) {
        clearTimeout(probingWatchdogTimer.current);
        probingWatchdogTimer.current = null;
      }
    };
  }, [state.cmsVerifySubstate, dispatch]);

  // ── Verify handler ─────────────────────────────────────────────────────
  // Only reachable when cmsVerifySubstate === 'idle' (button absent otherwise).
  const handleVerify = useCallback(async () => {
    if (state.cmsVerifySubstate !== 'idle') return;

    dispatch({ type: 'BEGIN_CMS_VERIFY' });

    try {
      await invoke<void>('verify_cms_connection');
      dispatch({ type: 'CMS_VERIFY_RESULT', ok: true });
    } catch (err) {
      // Busy means the Rust single-flight mutex was contended — a prior
      // verify_cms_connection is still in flight. Treat as NO-OP: do NOT
      // dispatch CMS_VERIFY_RESULT, do NOT flip the UI to `error`.
      if (errorKind(err) === 'Busy') return;

      // Unexpected command-level error: surface as error substate so the
      // operator is never stuck in `probing`.
      dispatch({
        type: 'CMS_VERIFY_RESULT',
        ok: false,
        errorMessage: `Unexpected error: ${extractErrorDetail(err)}`,
      });
    }
  }, [state.cmsVerifySubstate, dispatch]);

  // ── Skip handler ───────────────────────────────────────────────────────
  const handleSkip = useCallback(() => {
    dispatch({ type: 'SKIP_CMS_VERIFY' });
  }, [dispatch]);

  // ── Retry handler ─────────────────────────────────────────────────────
  // RETRY_CMS_VERIFY transitions error → probing in the reducer BEFORE the invoke,
  // so React leaves `error` and enters `probing` at the moment of invoke. The
  // [Retry] button is rendered ONLY in `error`, so once in `probing` there is no
  // activation surface for a second connection — preserving the dedup invariant.
  const handleRetry = useCallback(async () => {
    if (state.cmsVerifySubstate !== 'error') return;

    dispatch({ type: 'RETRY_CMS_VERIFY' });

    try {
      await invoke<void>('verify_cms_connection');
      dispatch({ type: 'CMS_VERIFY_RESULT', ok: true });
    } catch (err) {
      if (errorKind(err) === 'Busy') return;

      dispatch({
        type: 'CMS_VERIFY_RESULT',
        ok: false,
        errorMessage: `Unexpected error on retry: ${extractErrorDetail(err)}`,
      });
    }
  }, [state.cmsVerifySubstate, dispatch]);

  // ── Edit credentials handler ──────────────────────────────────────────
  const handleEditCredentials = useCallback(() => {
    dispatch({ type: 'RETURN_TO_CREDENTIALS' });
  }, [dispatch]);

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
      {state.cmsVerifySubstate === 'idle' && (
        <div data-testid="substate-idle">
          <h1>Verify your CMS connection</h1>
          <p>
            Ready to verify your CMS credentials. This opens a connection to the
            CMS and confirms your account is reachable — no message is sent.
          </p>
          {/* [Verify CMS Connection] button is PRESENT only in idle substate.
              It is ABSENT (not rendered) in probing/ok/error — dedup invariant. */}
          <div className="wizard-submit-row">
            <button
              type="button"
              data-testid="send-test-btn"
              onClick={handleVerify}
            >
              Verify CMS Connection
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

      {/* ── probing substate ──────────────────────────────────────────── */}
      {state.cmsVerifySubstate === 'probing' && (
        <div data-testid="substate-sending">
          <h1>Connecting to CMS…</h1>
          {/* Session-log preview — lines appended via CMS_VERIFY_LOG_LINE */}
          <div
            className="wizard-session-log"
            role="log"
            aria-live="polite"
            data-testid="session-log"
          >
            {state.cmsVerifyLog.length === 0 ? (
              <p className="wizard-log-placeholder">Connecting to CMS via TLS (port 8773)…</p>
            ) : (
              state.cmsVerifyLog.map((line, i) => (
                // eslint-disable-next-line react/no-array-index-key
                <p key={i} className="wizard-log-line">{line}</p>
              ))
            )}
          </div>
          {/* [Skip and go to inbox] — always available during probing per spec §3.4 */}
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
          {/* Note: [Verify CMS Connection] button is ABSENT here — dedup invariant */}
        </div>
      )}

      {/* ── ok substate ───────────────────────────────────────────────── */}
      {state.cmsVerifySubstate === 'ok' && (
        <div data-testid="substate-success">
          <span
            className="wizard-success-icon"
            role="img"
            aria-label="Success"
            data-testid="success-icon"
          >
            ✓
          </span>
          <h1>CMS connection verified.</h1>
          <p data-testid="success-message">Your CMS account is verified.</p>
          <p className="wizard-auto-advance-hint">
            Continuing to inbox in 3 seconds…{' '}
            <button
              type="button"
              onClick={() => {
                if (autoAdvanceTimer.current !== null) {
                  clearTimeout(autoAdvanceTimer.current);
                  autoAdvanceTimer.current = null;
                }
                dispatch({ type: 'SKIP_CMS_VERIFY' });
              }}
              data-testid="go-to-inbox-now-btn"
              className="wizard-btn-link"
            >
              Go to inbox now
            </button>
          </p>
          {/* Note: [Verify CMS Connection] button ABSENT — dedup invariant */}
        </div>
      )}

      {/* ── error substate ────────────────────────────────────────────── */}
      {state.cmsVerifySubstate === 'error' && (
        <div data-testid="substate-failed">
          <span
            className="wizard-warning-icon"
            role="img"
            aria-label="Warning"
            data-testid="warning-icon"
          >
            ⚠
          </span>
          <h1>CMS connection did not complete.</h1>
          {/* Yellow warning copy — NOT red error (spec §3.4: "failure is information, not a wall") */}
          <p className="wizard-failed-warning" data-testid="failed-message">
            Your credentials are saved. You can reach your inbox now and retry the
            verification later from <strong>Session → Test send</strong>.
          </p>
          {/* Specific error from cmsVerifyError */}
          {state.cmsVerifyError && (
            <p
              className="wizard-failed-detail"
              role="alert"
              data-testid="failed-error"
            >
              Error: {state.cmsVerifyError}
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
            {/* [Retry] — re-invokes verify_cms_connection; see handleRetry above */}
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
            {/* [Go to inbox] — dispatches SKIP_CMS_VERIFY */}
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
          {/* Note: [Verify CMS Connection] button ABSENT — dedup invariant */}
        </div>
      )}
    </div>
  );
}
