/**
 * VerifyCmsDialog — inline "Verify CMS Connection" overlay (tuxlink-lqw2).
 *
 * Opened from Tools → Verify CMS Connection. Runs the connect-only
 * NativeBackend probe (verify_cms_connection): opens a real CMS session over
 * internet telnet (CMS-SSL, port 8773 by default), confirms reachability +
 * auth, defers any offered mail, and sends nothing. No transmission — RADIO-1
 * does not apply to this internet path (see ADR 0018 + the wizard Step 3 probe
 * it shares a backend with).
 *
 * NOT a separate OS window — inline overlay per feedback_inline_ui_no_window_clutter.
 * Mirrors AboutDialog's backdrop/panel chrome and the wizard Step 3 probe
 * semantics (ok / error / Busy single-flight + watchdog).
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './VerifyCmsDialog.css';

// The backend probe has no built-in timeout (the NativeBackend connect uses OS
// TCP defaults, ~75 s for SYN retries). A generous watchdog keeps the dialog
// from hanging in "probing" forever. Mirrors the wizard Step 3 watchdog
// (tuxlink-9w8 pattern): 75 s OS bound + 15 s margin.
const PROBING_WATCHDOG_MS = 90_000;

type Substate = 'probing' | 'ok' | 'error';

// Tauri rejects the invoke promise with the serialized WizardError object.
// Read its discriminant `kind` (e.g. 'Busy' = single-flight mutex contended).
function errorKind(err: unknown): string | null {
  if (typeof err === 'object' && err !== null && 'kind' in err) {
    return String((err as { kind: unknown }).kind);
  }
  return null;
}

// Extract a human-readable detail from a caught WizardError or unknown error
// (mirrors Step3TestSend.extractErrorDetail).
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

export interface VerifyCmsDialogProps {
  open: boolean;
  onClose: () => void;
}

export function VerifyCmsDialog({ open, onClose }: VerifyCmsDialogProps) {
  const [substate, setSubstate] = useState<Substate>('probing');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const watchdog = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Guards a duplicate probe within one mount (e.g. React 18 StrictMode's
  // double-effect on open). The backend single-flight mutex would reject the
  // second with 'Busy', but not re-firing is cleaner.
  const inFlight = useRef(false);

  const runProbe = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setSubstate('probing');
    setErrorMessage(null);
    try {
      await invoke<void>('verify_cms_connection');
      setSubstate('ok');
    } catch (err) {
      if (errorKind(err) === 'Busy') {
        setErrorMessage('A verification is already in progress. Try again in a moment.');
      } else {
        setErrorMessage(extractErrorDetail(err));
      }
      setSubstate('error');
    } finally {
      inFlight.current = false;
    }
  }, []);

  // Auto-run the probe when the dialog opens.
  useEffect(() => {
    if (!open) return;
    void runProbe();
  }, [open, runProbe]);

  // Watchdog: bail out of "probing" if the backend never resolves.
  useEffect(() => {
    if (substate !== 'probing') return;
    watchdog.current = setTimeout(() => {
      watchdog.current = null;
      inFlight.current = false;
      setErrorMessage('Connection timed out — the CMS did not respond in time. Try again.');
      setSubstate('error');
    }, PROBING_WATCHDOG_MS);
    return () => {
      if (watchdog.current !== null) {
        clearTimeout(watchdog.current);
        watchdog.current = null;
      }
    };
  }, [substate]);

  // Esc closes (matches AboutDialog / SettingsPanel).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="tux-vcms-backdrop"
      data-testid="verify-cms-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-vcms-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Verify CMS Connection"
        data-testid="verify-cms-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-vcms-header">
          <h2 className="tux-vcms-title">Verify CMS Connection</h2>
          <button
            type="button"
            className="tux-vcms-close"
            data-testid="verify-cms-close"
            aria-label="Close Verify CMS dialog"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        <div className="tux-vcms-body">
          <p className="tux-vcms-intro">
            Opens a connection to the CMS to confirm your account is reachable and
            your credentials are accepted. CMS-SSL (TLS-encrypted, port 8773) over
            the internet — no message is sent and no radio is keyed.
          </p>

          {substate === 'probing' && (
            <div
              className="tux-vcms-status tux-vcms-probing"
              data-testid="verify-cms-probing"
              role="status"
              aria-live="polite"
            >
              <span className="tux-vcms-spinner" aria-hidden="true" />
              <span>Connecting to CMS…</span>
            </div>
          )}

          {substate === 'ok' && (
            <div
              className="tux-vcms-status tux-vcms-ok"
              data-testid="verify-cms-ok"
              role="status"
            >
              <span className="tux-vcms-ok-icon" role="img" aria-label="Success">✓</span>
              <div>
                <p className="tux-vcms-result-head">CMS connection verified.</p>
                <p className="tux-vcms-result-sub">
                  Your account is reachable and your credentials were accepted.
                </p>
              </div>
            </div>
          )}

          {substate === 'error' && (
            <div className="tux-vcms-status tux-vcms-error" data-testid="verify-cms-error">
              <span className="tux-vcms-error-icon" role="img" aria-label="Warning">⚠</span>
              <div>
                <p className="tux-vcms-result-head">CMS connection did not complete.</p>
                {errorMessage && (
                  <p
                    className="tux-vcms-error-detail"
                    role="alert"
                    data-testid="verify-cms-error-detail"
                  >
                    {errorMessage}
                  </p>
                )}
                <p className="tux-vcms-causes-heading">Likely causes:</p>
                <ul className="tux-vcms-causes">
                  <li>No internet connection</li>
                  <li>Firewall blocking port 8773</li>
                  <li>Incorrect callsign or password</li>
                  <li>CMS temporarily busy, or a captive portal intercepting traffic</li>
                </ul>
              </div>
            </div>
          )}
        </div>

        <div className="tux-vcms-actions">
          {substate === 'error' && (
            <button
              type="button"
              className="tux-vcms-button tux-vcms-button-primary"
              data-testid="verify-cms-retry"
              onClick={() => void runProbe()}
            >
              Retry
            </button>
          )}
          <button
            type="button"
            className="tux-vcms-button"
            data-testid="verify-cms-done"
            onClick={onClose}
          >
            {substate === 'ok' ? 'Done' : 'Close'}
          </button>
        </div>
      </div>
    </div>
  );
}
