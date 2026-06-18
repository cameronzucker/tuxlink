// CmsRecoveryEmail.tsx — set/replace the Winlink CMS account recovery email
// (tuxlink-vfb3 sub-project 3). Requires the current password as proof; sends
// account_set_recovery_email. No keyring effect.
//
// Recovery email is what lets a user recover a forgotten password (Winlink emails
// it to this address). Keeping it current is the single biggest lever on the
// "locked out, no recovery address" support burden — so it is editable here, not
// only at account creation.
//
// Gated on TUXLINK_WINLINK_ACCESS_CODE (cms_password_change_available): renders
// nothing in the open build. RADIO-1: internet HTTPS, not a transmission.

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { validateRecoveryEmail } from '../wizard/validators';

export interface CmsRecoveryEmailProps {
  /** The account callsign whose recovery email is being set (active identity). */
  callsign: string;
}

interface BackendError {
  kind?: string;
  code?: string;
  message?: string;
}

function setRecoveryError(be: BackendError): string {
  switch (be?.kind) {
    case 'InvalidKey':
      return 'This Tuxlink build is not configured for Winlink account management.';
    case 'Rejected':
      return be.message ?? 'Winlink rejected the change (check your current password).';
    case 'UnknownOutcome':
      return 'The request timed out before the result could be confirmed. Try again.';
    case 'Network':
      return 'Could not reach the Winlink account service. Check your connection and try again.';
    case 'NotConfigured':
      return 'Recovery-email management is unavailable on this build.';
    default:
      return be?.message ?? 'Could not update the recovery email.';
  }
}

export function CmsRecoveryEmail({ callsign }: CmsRecoveryEmailProps) {
  const [available, setAvailable] = useState(false);
  const [password, setPassword] = useState('');
  const [email, setEmail] = useState('');
  const [inFlight, setInFlight] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    let active = true;
    invoke<boolean>('cms_password_change_available')
      .then((v) => {
        if (active) setAvailable(Boolean(v));
      })
      .catch(() => {
        if (active) setAvailable(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const emailError = validateRecoveryEmail(email);
  const passwordError = password === '' ? 'Enter your current password.' : null;
  const canSubmit = !emailError && !passwordError && !inFlight;

  const onSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setError(null);
    setSuccess(false);
    setInFlight(true);
    try {
      await invoke('cms_account_set_recovery_email', {
        rawCallsign: callsign,
        password,
        recoveryEmail: email,
      });
      setPassword('');
      setSuccess(true);
    } catch (e) {
      setError(setRecoveryError(e as BackendError));
    } finally {
      setInFlight(false);
    }
  }, [canSubmit, callsign, password, email]);

  if (!available) return null;

  return (
    <section className="wizard-field" data-testid="account-recovery-email">
      <h2>Recovery email</h2>
      <p className="wizard-hint">
        Set the address Winlink emails your password to if you forget it. Requires your
        current CMS password to confirm the change.
      </p>

      <div className="wizard-field">
        <label htmlFor="account-recovery-new">New recovery email</label>
        <input
          id="account-recovery-new"
          data-testid="account-recovery-new"
          type="email"
          autoComplete="email"
          placeholder="you@example.com"
          value={email}
          onChange={(e) => {
            setEmail(e.target.value);
            setError(null);
            setSuccess(false);
          }}
          disabled={inFlight}
        />
        {email !== '' && emailError && (
          <span role="alert" className="wizard-field-error">
            {emailError}
          </span>
        )}
      </div>

      <div className="wizard-field">
        <label htmlFor="account-recovery-password">Current password for {callsign}</label>
        <input
          id="account-recovery-password"
          data-testid="account-recovery-password"
          type="password"
          autoComplete="current-password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
            setError(null);
            setSuccess(false);
          }}
          disabled={inFlight}
        />
      </div>

      {error && (
        <div role="alert" className="wizard-field-error" data-testid="account-recovery-error">
          {error}
        </div>
      )}
      {success && (
        <div role="status" className="wizard-success" data-testid="account-recovery-success">
          Recovery email updated.
        </div>
      )}

      <button
        type="button"
        data-testid="account-recovery-submit"
        disabled={!canSubmit}
        onClick={onSubmit}
      >
        {inFlight ? 'Saving…' : 'Update recovery email'}
      </button>
    </section>
  );
}
