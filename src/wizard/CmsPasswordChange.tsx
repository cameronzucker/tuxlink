// CmsPasswordChange.tsx — in-wizard CMS account password rotation (tuxlink-vfb3).
//
// Lets a user rotate their Winlink CMS account password without Winlink Express.
// The backend (cms_account_password_change) reads the CURRENT password from the
// keyring as the OldPassword proof, POSTs the change to the account API, and on
// success updates the keyring atomically; this control only collects the NEW
// password (+ confirm).
//
// Gating: the feature requires an injected access code (TUXLINK_WINLINK_ACCESS_CODE).
// When it's absent, cms_password_change_available() is false and this renders
// NOTHING — the open build never shows a control that can't work.

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { validatePassword, cmsPasswordTruncationNotice } from './validators';

export interface CmsPasswordChangeProps {
  /** The account callsign whose CMS password is being rotated. */
  callsign: string;
}

interface BackendError {
  kind?: string;
  message?: string;
}

export function CmsPasswordChange({ callsign }: CmsPasswordChangeProps) {
  const [available, setAvailable] = useState(false);
  const [current, setCurrent] = useState('');
  const [newPw, setNewPw] = useState('');
  const [confirm, setConfirm] = useState('');
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

  const currentError = current === '' ? 'Enter your current password.' : null;
  const newError = validatePassword(newPw);
  const newTruncationNotice = cmsPasswordTruncationNotice(newPw);
  const matchError = confirm !== newPw ? 'Passwords do not match.' : null;
  const canSubmit = !currentError && !newError && !matchError && !inFlight;

  const onSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setError(null);
    setSuccess(false);
    setInFlight(true);
    try {
      await invoke('cms_password_change', {
        rawCallsign: callsign,
        oldPassword: current,
        newPassword: newPw,
      });
      // Clear all secrets from the DOM immediately on success.
      setCurrent('');
      setNewPw('');
      setConfirm('');
      setSuccess(true);
    } catch (e) {
      const be = e as BackendError;
      // Surface the CMS's own message verbatim (Rejected.message); fall back for
      // transport / not-configured / keyring-desync variants without a message.
      setError(be?.message ?? errorForKind(be?.kind));
    } finally {
      setInFlight(false);
    }
  }, [canSubmit, callsign, current, newPw]);

  if (!available) return null;

  return (
    <section className="wizard-field cms-password-change" data-testid="cms-password-change">
      <h2>Change CMS password</h2>
      <p className="wizard-hint">
        Choose a <strong>unique</strong> password you don&rsquo;t reuse anywhere else. Winlink can
        email this password to you in plaintext on request, so it should never be one that protects
        another account.
      </p>

      <div className="wizard-field">
        <label htmlFor="cms-pw-current">Current password</label>
        <input
          id="cms-pw-current"
          data-testid="cms-pw-current"
          type="password"
          autoComplete="current-password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          disabled={inFlight}
        />
      </div>

      <div className="wizard-field">
        <label htmlFor="cms-pw-new">New password</label>
        <input
          id="cms-pw-new"
          data-testid="cms-pw-new"
          type="password"
          autoComplete="new-password"
          value={newPw}
          onChange={(e) => setNewPw(e.target.value)}
          disabled={inFlight}
        />
        {newPw !== '' && newError && (
          <span role="alert" className="wizard-field-error">
            {newError}
          </span>
        )}
        {newTruncationNotice && (
          <span className="wizard-field-notice" data-testid="cms-pw-new-truncation-notice">
            {newTruncationNotice}
          </span>
        )}
      </div>

      <div className="wizard-field">
        <label htmlFor="cms-pw-confirm">Confirm new password</label>
        <input
          id="cms-pw-confirm"
          data-testid="cms-pw-confirm"
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          disabled={inFlight}
        />
        {confirm !== '' && matchError && (
          <span role="alert" className="wizard-field-error">
            {matchError}
          </span>
        )}
      </div>

      {error && (
        <div role="alert" className="wizard-field-error" data-testid="cms-pw-error">
          {error}
        </div>
      )}
      {success && (
        <div role="status" className="wizard-success" data-testid="cms-pw-success">
          CMS password changed and saved to your keyring.
        </div>
      )}

      <button type="button" data-testid="cms-pw-submit" disabled={!canSubmit} onClick={onSubmit}>
        {inFlight ? 'Changing…' : 'Change password'}
      </button>
    </section>
  );
}

/** Fallback message for error variants the backend returns without a message. */
function errorForKind(kind?: string): string {
  switch (kind) {
    case 'NotConfigured':
      return 'Password change is not available on this build.';
    case 'InvalidKey':
      return 'The Winlink access key is missing or not valid for this operation. Password change is not available until a valid key is configured.';
    case 'KeyringDesync':
      return 'The CMS password was changed, but saving it to your keyring failed — re-enter your credentials to resync.';
    case 'Network':
      return 'Could not reach the Winlink account service. Check your connection and try again.';
    case 'UnknownOutcome':
      return 'The request timed out before we could confirm the result. Your password may or may not have changed — verify before trying again.';
    default:
      return 'Password change failed.';
  }
}
