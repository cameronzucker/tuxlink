// src/radio/sections/AuthDiagnosticBanner.tsx
//
// Smart Auth-Failure Diagnostic Banner (tuxlink-7do4, spec §4 + §4.5 + §8.2).
//
// Consumes useAuthDiagnostic() and renders the appropriate failure-mode banner
// with per-mode copy, recovery affordances, and common controls (wire-response
// toggle, dismiss, copy-log). Inserted into TelnetRadioPanel in Task 22.
//
// Security invariants:
//   - Wire response renders in a plain <pre> node — NO dangerouslySetInnerHTML.
//   - Password state cleared in finally (R2 #3 + R5 §4.3 i).
//   - All outbound links go through shellOpen (never in-webview navigation).

import { useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { cmsPasswordTruncationNotice } from '../../wizard/validators';
import { useAuthDiagnostic } from '../../connections/useAuthDiagnostic';
import { useSessionLog } from './useSessionLog';
import { copyFor } from './authDiagnosticCopy';
import {
  WINLINK_ORG_PASSWORD_RESET_URL,
  WINLINK_ORG_ACCOUNT_URL,
  TUXLINK_GITHUB_ISSUE_NEW_URL,
} from '../../connections/winlinkOrgUrls';
import type { FailureMode } from '../../connections/sessionTypes';
import './AuthDiagnosticBanner.css';

// ---------------------------------------------------------------------------
// Ordinal helper (for retry counter)
// ---------------------------------------------------------------------------

function ordinal(n: number): string {
  const s = ['th', 'st', 'nd', 'rd'];
  const v = n % 100;
  return n + (s[(v - 20) % 10] ?? s[v] ?? s[0]);
}

// ---------------------------------------------------------------------------
// Inline Re-enter Password Form (Mode 3 only)
// ---------------------------------------------------------------------------

interface PasswordFormProps {
  onClose: () => void;
}

function PasswordForm({ onClose }: PasswordFormProps) {
  // Password is local state scoped to this form component — it is cleared
  // in `finally` so it never survives the synchronous invoke call (R2 #3).
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
    // Enter submits via the form's onSubmit; no additional handling needed.
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);

    // Capture password into a local variable before the await so we can
    // clear the state in finally without a stale-closure issue.
    const pw = password;
    try {
      // Retrieve current callsign from config (same pattern as TelnetRadioPanel).
      const config = await invoke<{ callsign?: string }>('config_read');
      const callsign = config.callsign ?? '';
      await invoke('credentials_write_password', { callsign, password: pw });
      onClose();
    } catch {
      setError('Keyring unavailable — unlock + try again.');
    } finally {
      // R2 #3 + R5 §4.3 i: password must not persist in React state beyond
      // the synchronous invoke boundary. Clear regardless of success/failure.
      setPassword('');
      setSaving(false);
    }
  };

  return (
    <form
      className="diag-form"
      data-testid="diag-password-form"
      onSubmit={(e) => void handleSubmit(e)}
    >
      <div className="row-input">
        <span>Password</span>
        <input
          type="password"
          value={password}
          autoFocus
          placeholder="Winlink password"
          autoComplete="current-password"
          disabled={saving}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={handleKeyDown}
          data-testid="diag-password-input"
        />
      </div>
      {cmsPasswordTruncationNotice(password) && (
        <p
          className="diag-help"
          data-testid="diag-password-truncation-notice"
          style={{ color: 'var(--tux-warn, #ffd166)' }}
        >
          {cmsPasswordTruncationNotice(password)}
        </p>
      )}
      {error && (
        <p className="diag-help" data-testid="diag-password-error" style={{ color: 'var(--error)' }}>
          {error}
        </p>
      )}
      <div className="btn-row">
        <button
          type="submit"
          className="diag-btn diag-btn-primary"
          data-testid="diag-password-save"
          disabled={saving || !password}
        >
          {saving ? 'Saving…' : 'Save to keyring'}
        </button>
        <button
          type="button"
          className="diag-btn diag-btn-secondary"
          data-testid="diag-password-cancel"
          onClick={onClose}
        >
          Cancel
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// "Check this password works" button (Modes 3 + 5)
// ---------------------------------------------------------------------------

interface TestCredentialsButtonProps {
  testingInFlight: boolean;
  disabledUntil: number | null;
  circuitBroken: boolean;
  onTest: () => void;
}

function TestCredentialsButton({
  testingInFlight,
  disabledUntil,
  circuitBroken,
  onTest,
}: TestCredentialsButtonProps) {
  const now = Date.now();
  const rateLimited = (disabledUntil !== null && disabledUntil > now) || circuitBroken;

  if (testingInFlight) {
    return (
      <span
        className="diag-btn diag-btn-secondary"
        data-testid="diag-testing-indicator"
        aria-live="polite"
        aria-label="Testing credentials"
      >
        <span className="diag-icon spinning" aria-hidden="true">⟳</span>
        {' '}Testing…
      </span>
    );
  }

  const tooltip = circuitBroken
    ? 'Multiple retries — wait 2 minutes.'
    : disabledUntil !== null && disabledUntil > now
      ? `Rate limited — wait ${Math.ceil((disabledUntil - now) / 1000)}s.`
      : undefined;

  return (
    <button
      type="button"
      className="diag-btn diag-btn-secondary"
      data-testid="diag-test-credentials-btn"
      disabled={rateLimited}
      title={tooltip}
      onClick={onTest}
    >
      Check this password works
    </button>
  );
}

// ---------------------------------------------------------------------------
// "Copy log for help" button
// ---------------------------------------------------------------------------

interface CopyLogButtonProps {
  rawWireResponse: string | null;
  entries: { ts: string; level: string; message: string }[];
}

function CopyLogButton({ rawWireResponse, entries }: CopyLogButtonProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    const logText = entries.map((e) => `[${e.ts}] [${e.level}] ${e.message}`).join('\n');
    const parts = [logText];
    if (rawWireResponse) {
      parts.push('\n--- Wire Response ---\n' + rawWireResponse);
    }
    try {
      await navigator.clipboard.writeText(parts.join('\n'));
      setCopied(true);
      setTimeout(() => setCopied(false), 8000);
    } catch {
      // clipboard API unavailable (non-HTTPS context in test env); swallow.
    }
  };

  if (copied) {
    return (
      <span
        className="diag-help"
        data-testid="diag-copy-confirmation"
        aria-live="polite"
        style={{ fontSize: '11px', color: 'var(--text-dim, #94a3b8)' }}
      >
        Log copied — sensitive tokens redacted. Paste into a GitHub issue at
        github.com/cameronzucker/tuxlink/issues or share with help channels.
      </span>
    );
  }

  return (
    <button
      type="button"
      className="diag-btn diag-btn-secondary"
      data-testid="diag-copy-log-btn"
      onClick={() => void handleCopy()}
    >
      Copy log for help
    </button>
  );
}

// ---------------------------------------------------------------------------
// Wire response toggle
// ---------------------------------------------------------------------------

interface WireResponseToggleProps {
  rawWireResponse: string;
}

function WireResponseToggle({ rawWireResponse }: WireResponseToggleProps) {
  const [open, setOpen] = useState(false);

  return (
    <div className="diag-raw">
      <button
        type="button"
        className={`diag-raw-toggle${open ? ' open' : ''}`}
        data-testid="diag-raw-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {open ? 'Hide wire response' : 'Show wire response'}
      </button>
      {open && (
        // Plain-text rendering only — no dangerouslySetInnerHTML (R2 #5).
        <pre className="diag-raw-content" data-testid="diag-raw-content">
          {rawWireResponse}
        </pre>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Mode icon helper
// ---------------------------------------------------------------------------

function iconFor(mode: FailureMode | null): { glyph: string; cls: string } {
  if (mode === 'uncategorized') return { glyph: '⚠', cls: 'uncat' };
  return { glyph: '✕', cls: 'bad' };
}

// ---------------------------------------------------------------------------
// Banner variant class
// ---------------------------------------------------------------------------

function bannerClassFor(mode: FailureMode | null): string {
  if (mode === 'uncategorized') return 'diag-banner uncat';
  return 'diag-banner';
}

// ---------------------------------------------------------------------------
// AuthDiagnosticBanner (main export)
// ---------------------------------------------------------------------------

export function AuthDiagnosticBanner() {
  const { state, dismiss, testCredentials } = useAuthDiagnostic();
  const { entries } = useSessionLog();
  const [showPasswordForm, setShowPasswordForm] = useState(false);

  // Stable reference for dismiss so callback isn't recreated every render.
  const dismissRef = useRef(dismiss);
  dismissRef.current = dismiss;

  if (state.mode === null) return null;

  const { mode, retryCount, rawWireResponse, transportKind, testingInFlight, testRateLimit } =
    state;

  const { headline, body } = copyFor({ mode, transportKind });

  // Append retry counter to headline when retryCount > 1 (R4 #15).
  const displayHeadline =
    retryCount > 1 ? `${headline} (${ordinal(retryCount)} attempt)` : headline;

  const { glyph, cls: iconCls } = iconFor(mode);
  const bannerCls = bannerClassFor(mode);

  const openShellLink = (url: string) => () => {
    void shellOpen(url).catch(() => {
      // shell-open failure is silent — operator still sees the button.
    });
  };

  return (
    <div
      className={bannerCls}
      role="alert"
      aria-live="polite"
      data-testid="diag-banner"
      data-mode={mode}
    >
      {/* Header row */}
      <div className="diag-head">
        <span className={`diag-icon ${iconCls}`} aria-hidden="true">
          {glyph}
        </span>
        <span className="diag-title" data-testid="diag-title">
          {displayHeadline}
        </span>
        <button
          type="button"
          className="diag-dismiss"
          data-testid="diag-dismiss"
          aria-label="Dismiss diagnostic banner"
          onClick={() => void dismissRef.current()}
        >
          ×
        </button>
      </div>

      {/* Body copy */}
      <p className="diag-body" data-testid="diag-body">
        {body}
      </p>

      {/* Action buttons — per-mode affordances */}
      <div className="diag-actions">
        {/* Mode 2: client_rejected */}
        {mode === 'client_rejected' && (
          <>
            <button
              type="button"
              className="diag-btn diag-btn-secondary"
              data-testid="diag-switch-cmsz-btn"
              onClick={() => {
                void invoke('config_set_connect', {
                  host: 'cms-z.winlink.org',
                  transport: 'Telnet',
                }).catch(() => {});
              }}
            >
              Switch to cms-z (dev)
            </button>
            <button
              type="button"
              className="diag-btn diag-btn-link"
              data-testid="diag-issue-tracker-btn"
              onClick={openShellLink(TUXLINK_GITHUB_ISSUE_NEW_URL)}
            >
              Open issue tracker
            </button>
          </>
        )}

        {/* Mode 3: password_rejected */}
        {mode === 'password_rejected' && (
          <>
            <button
              type="button"
              className="diag-btn diag-btn-primary"
              data-testid="diag-reenter-password-btn"
              onClick={() => setShowPasswordForm((v) => !v)}
            >
              Re-enter password
            </button>
            <TestCredentialsButton
              testingInFlight={testingInFlight}
              disabledUntil={testRateLimit.disabledUntil}
              circuitBroken={testRateLimit.circuitBroken}
              onTest={() => void testCredentials()}
            />
            <button
              type="button"
              className="diag-btn diag-btn-link"
              data-testid="diag-reset-password-btn"
              onClick={openShellLink(WINLINK_ORG_PASSWORD_RESET_URL)}
            >
              Reset on winlink.org (in browser)
            </button>
          </>
        )}

        {/* Mode 4: callsign_rejected — PRIMARY first per R4 #2 */}
        {mode === 'callsign_rejected' && (
          <>
            <button
              type="button"
              className="diag-btn diag-btn-link diag-btn-primary"
              data-testid="diag-verify-callsign-btn"
              onClick={openShellLink(WINLINK_ORG_ACCOUNT_URL)}
            >
              Verify on winlink.org (in browser)
            </button>
            <button
              type="button"
              className="diag-btn diag-btn-secondary"
              data-testid="diag-change-callsign-btn"
              onClick={() => {
                void invoke('wizard_reopen', { step: 'callsign' }).catch(() => {});
              }}
            >
              Try a different callsign
            </button>
          </>
        )}

        {/* Mode 5: session_dropped_after_auth */}
        {mode === 'session_dropped_after_auth' && (
          <TestCredentialsButton
            testingInFlight={testingInFlight}
            disabledUntil={testRateLimit.disabledUntil}
            circuitBroken={testRateLimit.circuitBroken}
            onTest={() => void testCredentials()}
          />
        )}

        {/* Uncategorized: "Try a different callsign" */}
        {mode === 'uncategorized' && (
          <button
            type="button"
            className="diag-btn diag-btn-secondary"
            data-testid="diag-change-callsign-btn"
            onClick={() => {
              void invoke('wizard_reopen', { step: 'callsign' }).catch(() => {});
            }}
          >
            Try a different callsign
          </button>
        )}

        {/* Common: Copy log for help (all modes) */}
        <CopyLogButton rawWireResponse={rawWireResponse} entries={entries} />

        {/* Common: Dismiss */}
        {/* NOTE: dismiss is also in the header row (×) — this slot is intentionally
            kept empty; per spec the × in the header IS the dismiss affordance.
            No duplicate dismiss button in the action row. */}
      </div>

      {/* Mode 3: inline re-enter password form (toggled) */}
      {mode === 'password_rejected' && showPasswordForm && (
        <PasswordForm onClose={() => setShowPasswordForm(false)} />
      )}

      {/* Wire response toggle (present when rawWireResponse is non-null) */}
      {rawWireResponse !== null && <WireResponseToggle rawWireResponse={rawWireResponse} />}
    </div>
  );
}
