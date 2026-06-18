// WinlinkAccountSettings — the Settings "Winlink Account" section (tuxlink-vfb3).
//
// Two operator surfaces for the active CMS account credential, both inline:
//   1. CmsPasswordChange — rotates the password ON the Winlink CMS server and
//      updates the keyring on success (gated on TUXLINK_WINLINK_ACCESS_CODE; it
//      renders nothing in the open build).
//   2. Re-enter password — a keyring-only recovery for the "KeyringDesync" case
//      (server password changed but the local keyring write failed). It writes
//      ONLY the keyring (credentials_write_password) for the ACTIVE identity; it
//      does NOT rewrite config.json, so grid / MBO / modem / APRS / favorites are
//      untouched. The callsign is fixed to the active identity (mycall) so a
//      password can never be stored under the wrong account.
//
// Inline, in-chrome (no popup windows; constrained widths via the section CSS).

import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CmsPasswordChange } from '../wizard/CmsPasswordChange';
import { validatePassword } from '../wizard/validators';

/** Mirrors the Rust ActiveIdentityDto returned by identity_active. */
interface ActiveIdentityDto {
  mycall: string;
  address_as: string;
  is_tactical: boolean;
}

export function WinlinkAccountSettings() {
  const [callsign, setCallsign] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  // Re-enter form state.
  const [password, setPassword] = useState('');
  const [inFlight, setInFlight] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    let active = true;
    invoke<ActiveIdentityDto | null>('identity_active')
      .then((id) => {
        if (active) setCallsign(id?.mycall ?? null);
      })
      .catch(() => {
        if (active) setCallsign(null);
      })
      .finally(() => {
        if (active) setLoaded(true);
      });
    return () => {
      active = false;
    };
  }, []);

  const pwError = validatePassword(password);
  const canSubmit = !pwError && !inFlight && !!callsign;

  const onReenter = useCallback(async () => {
    if (!canSubmit || !callsign) return;
    setError(null);
    setSuccess(false);
    setInFlight(true);
    try {
      await invoke('credentials_write_password', { callsign, password });
      setPassword(''); // clear the secret from the DOM on success
      setSuccess(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String((e as { message?: string })?.message ?? e);
      setError(msg || 'Could not save the password to your keyring.');
    } finally {
      setInFlight(false);
    }
  }, [canSubmit, callsign, password]);

  if (!loaded) return null;

  if (!callsign) {
    return (
      <div className="tux-account-settings" data-testid="winlink-account-settings">
        <p data-testid="account-none">
          No Winlink CMS account is configured. Add a CMS identity under{' '}
          <strong>Identities</strong> to manage its password here.
        </p>
      </div>
    );
  }

  return (
    <div className="tux-account-settings" data-testid="winlink-account-settings">
      <p className="tux-account-current">
        Signed-in account:{' '}
        <strong data-testid="account-current-callsign">{callsign}</strong>
      </p>

      {/* Rotate the CMS server password (gated on the access code; renders
       *  nothing in the open build). */}
      <CmsPasswordChange callsign={callsign} />

      {/* Keyring-only recovery: re-save the existing password to the OS keyring
       *  for this account. Use when Tuxlink can't authenticate after the CMS
       *  password was changed elsewhere (or the keyring write failed mid-change).
       *  Does NOT contact the CMS and does NOT touch any other setting. */}
      <section className="wizard-field" data-testid="account-reenter">
        <h2>Re-enter saved password</h2>
        <p className="wizard-hint">
          Re-save your current CMS password to this computer&rsquo;s keyring. Use this if Tuxlink
          can&rsquo;t sign in after the password was changed elsewhere. This updates only the stored
          password &mdash; nothing is sent to Winlink.
        </p>

        <div className="wizard-field">
          <label htmlFor="account-reenter-password">Password for {callsign}</label>
          <input
            id="account-reenter-password"
            data-testid="account-reenter-password"
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
          <div role="alert" className="wizard-field-error" data-testid="account-reenter-error">
            {error}
          </div>
        )}
        {success && (
          <div role="status" className="wizard-success" data-testid="account-reenter-success">
            Password saved to your keyring.
          </div>
        )}

        <button
          type="button"
          data-testid="account-reenter-submit"
          disabled={!canSubmit}
          onClick={onReenter}
        >
          {inFlight ? 'Saving…' : 'Save password'}
        </button>
      </section>
    </div>
  );
}
