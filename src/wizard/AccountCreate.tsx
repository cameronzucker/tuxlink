// AccountCreate.tsx — in-app Winlink account creation (tuxlink-vfb3 sub-project 1).
//
// Reached from Step2Credentials via the "Create a Winlink account" affordance (the
// familiar login-form pattern — NOT an up-front fork). Collects callsign + password +
// confirm + a MANDATORY recovery email, creates the CMS account via cms_account_create,
// persists the config identity via wizard_persist_cms, then joins the existing
// cms_verify → location → complete tail (ACCOUNT_CREATE_SUCCESS).
//
// Spec: docs/superpowers/specs/2026-06-17-cms-account-wizard-creation-design.md
//
// All live exercise is blocked on a Tuxlink-issued access key (tuxlink-lu7t); the
// command is wired and offline-correct. RADIO-1: this is internet HTTPS to the account
// API, not a transmission.

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { useWizard } from './wizardContext';
import { CredentialFields } from './CredentialFields';
import {
  validateAmateurCallsign,
  validateAccountPassword,
  validateRecoveryEmail,
} from './validators';
import type { WizardError } from './types';

// Winlink account registration URL — opened in the system browser (never a webview,
// spec §3.7). The keyless degraded state (no TUXLINK_WINLINK_ACCESS_CODE) routes real
// users here, since the in-app create call cannot authenticate to the CMS without the
// Tuxlink-issued Key (tuxlink-lu7t).
const WINLINK_REGISTER_URL = 'https://www.winlink.org/user/register';

// AccountApiError (the cms_account command error), serialized #[serde(tag = "kind")].
interface BackendError {
  kind?: string;
  code?: string;
  message?: string;
  field?: string;
  detail?: { detail?: string };
}

/** The base callsign (SSID/qualifier stripped, uppercased) for the MBO auto-fill. */
function baseCallsign(raw: string): string {
  return raw.trim().split(/[-.]/)[0].toUpperCase();
}

/** Message for a backend error that did not arrive with a server `message`. */
function fallbackMessage(kind?: string): string {
  switch (kind) {
    case 'NotConfigured':
    case 'InvalidKey':
      return 'Account creation is unavailable on this build.';
    case 'KeyringDesync':
      return 'The account was created, but saving the password to your keyring failed — re-enter your credentials from Settings to resync.';
    case 'Network':
      return 'Could not reach the Winlink account service. Check your connection and try again.';
    case 'UnknownOutcome':
      return 'The request timed out before the result could be confirmed. The account may or may not have been created — try signing in before creating it again.';
    // wizard_persist_cms (WizardError) shapes that can surface from step 2:
    case 'Unavailable':
      return 'The account was created, but no system keyring was found to store the password. Install one (e.g. gnome-keyring) and sign in.';
    case 'Locked':
      return 'The account was created, but your keyring is locked. Unlock it and sign in.';
    case 'ConfigWrite':
    case 'ConfigWriteAndRollbackFailed':
      return 'The account was created, but saving the configuration failed. Sign in with your new callsign and password to finish setup.';
    default:
      return 'Account creation failed.';
  }
}

/** A rejection whose code/message indicates the callsign is already registered. */
function isExistingCallsign(err: BackendError): boolean {
  if (err.kind !== 'Rejected') return false;
  const hay = `${err.code ?? ''} ${err.message ?? ''}`.toLowerCase();
  return hay.includes('exist') || hay.includes('already');
}

export function AccountCreate() {
  const { state, dispatch } = useWizard();

  const [callsign, setCallsign] = useState(state.callsign);
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [recoveryEmail, setRecoveryEmail] = useState('');
  const [callsignTouched, setCallsignTouched] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [existsCallsign, setExistsCallsign] = useState<string | null>(null);

  // Is in-app account creation usable on this build? It needs the injected CMS access
  // key (TUXLINK_WINLINK_ACCESS_CODE); without it the create call cannot authenticate,
  // so the dialog degrades to an honest note + the external winlink.org register link
  // rather than a form that fails on submit (tuxlink-6afw). null = probe pending.
  const [available, setAvailable] = useState<boolean | null>(null);
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

  const handleRegisterClick = useCallback((e: React.MouseEvent<HTMLAnchorElement>) => {
    e.preventDefault();
    shellOpen(WINLINK_REGISTER_URL).catch(console.error);
  }, []);

  const callsignError = validateAmateurCallsign(callsign);
  const passwordError = validateAccountPassword(password);
  const matchError = confirm !== password ? 'Passwords do not match.' : null;
  const recoveryError = validateRecoveryEmail(recoveryEmail);
  const canSubmit =
    !callsignError && !passwordError && !matchError && !recoveryError && !state.inFlight;

  const handleCallsignChange = useCallback(
    (value: string) => {
      setCallsign(value);
      // Keep the reducer's callsign current so "Sign in with this callsign" / "Back to
      // sign in" prefill the credentials form (RETURN_TO_CREDENTIALS reads state.callsign).
      dispatch({ type: 'SET_CREDENTIALS_FIELD', field: 'callsign', value });
      setSubmitError(null);
      setExistsCallsign(null);
    },
    [dispatch]
  );

  const onSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setSubmitError(null);
    setExistsCallsign(null);
    dispatch({ type: 'SUBMIT_BEGIN' });
    try {
      // 1) Create the CMS account (writes the keyring password on success).
      await invoke('cms_account_create', {
        rawCallsign: callsign,
        password,
        recoveryEmail,
      });
      // 2) Persist the config identity (callsign + auto-filled MBO). Grid is collected
      //    later in the Location step. The keyring re-write here is idempotent.
      await invoke('wizard_persist_cms', {
        rawCallsign: callsign,
        password,
        grid: '',
        mboAddress: `${baseCallsign(callsign)}@winlink.org`,
      });
      setPassword('');
      setConfirm('');
      dispatch({ type: 'ACCOUNT_CREATE_SUCCESS' });
    } catch (e) {
      const be = e as BackendError;
      if (isExistingCallsign(be)) {
        setExistsCallsign(baseCallsign(callsign));
      } else {
        setSubmitError(be?.message ?? fallbackMessage(be?.kind));
      }
      dispatch({ type: 'SUBMIT_FAILURE', error: (be as unknown as WizardError) });
    }
  }, [canSubmit, dispatch, callsign, password, recoveryEmail]);

  const signInWithCallsign = useCallback(() => {
    // state.callsign is already current (kept in sync on change); just return.
    dispatch({ type: 'RETURN_TO_CREDENTIALS' });
  }, [dispatch]);

  // Availability probe still in flight — hold the step frame without flashing either
  // the form or the degraded note.
  if (available === null) {
    return (
      <div className="wizard-step wizard-step-account-create" data-testid="wc-loading">
        <h1>Create a Winlink account</h1>
      </div>
    );
  }

  // Keyless build: the create call cannot authenticate to the CMS, so present an honest
  // note + the external winlink.org register link instead of a form that fails on submit
  // (tuxlink-6afw).
  if (!available) {
    return (
      <div className="wizard-step wizard-step-account-create">
        <h1>Create a Winlink account</h1>
        <p data-testid="wc-unavailable">
          In-app account creation requires a Winlink CMS access key this build does not
          include. To create a Winlink account, register on winlink.org, then return here
          and sign in.
        </p>
        <p className="wizard-create-line">
          <a
            href={WINLINK_REGISTER_URL}
            onClick={handleRegisterClick}
            role="link"
            data-testid="wc-register-external"
            aria-label="Register a Winlink account"
          >
            Register on winlink.org
          </a>{' '}
          <span className="wizard-field-hint wizard-inline-hint">(opens your browser)</span>
        </p>
        <div className="wizard-submit-row">
          <button
            type="button"
            className="wizard-btn-secondary"
            data-testid="wc-back-to-signin"
            onClick={() => dispatch({ type: 'RETURN_TO_CREDENTIALS' })}
          >
            Back to sign in
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="wizard-step wizard-step-account-create">
      <h1>Create a Winlink account</h1>
      <p>
        Register your callsign with the Winlink CMS. This is free and takes a moment.
        Choose a unique password you do not reuse anywhere else.
      </p>

      <form noValidate onSubmit={(e) => e.preventDefault()}>
        <CredentialFields
          callsign={callsign}
          password={password}
          onCallsignChange={handleCallsignChange}
          onPasswordChange={(v) => {
            setPassword(v);
            setSubmitError(null);
          }}
          onCallsignBlur={() => setCallsignTouched(true)}
          callsignError={callsignTouched ? callsignError : null}
          disabled={state.inFlight}
          idPrefix="wc"
          callsignLabel="Callsign *"
          passwordLabel="Password *"
          passwordAutoComplete="new-password"
        />

        <div className="wizard-field-hint" data-testid="wc-pw-hint">
          6 to 12 characters.
        </div>

        <div className="wizard-field">
          <label htmlFor="wc-confirm">Confirm password *</label>
          <input
            id="wc-confirm"
            data-testid="wc-confirm"
            type="password"
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            disabled={state.inFlight}
          />
          {confirm !== '' && matchError && (
            <span role="alert" className="wizard-field-error">
              {matchError}
            </span>
          )}
        </div>

        <div className="wizard-field">
          <label htmlFor="wc-recovery">Recovery email *</label>
          <input
            id="wc-recovery"
            data-testid="wc-recovery"
            type="email"
            autoComplete="email"
            placeholder="you@example.com"
            value={recoveryEmail}
            onChange={(e) => {
              setRecoveryEmail(e.target.value);
              setSubmitError(null);
            }}
            disabled={state.inFlight}
          />
          <div className="wizard-field-hint">
            Required. If you forget your password, Winlink emails it to this address — so
            use one you control and a password you reuse nowhere else.
          </div>
          {recoveryEmail !== '' && recoveryError && (
            <span role="alert" className="wizard-field-error">
              {recoveryError}
            </span>
          )}
        </div>

        {existsCallsign && (
          <div role="alert" className="wizard-error-banner" data-testid="wc-exists">
            <strong>{existsCallsign} already has a Winlink account.</strong> If it is
            yours, sign in instead.{' '}
            <button type="button" className="wizard-linklike" onClick={signInWithCallsign}>
              Sign in with this callsign
            </button>
          </div>
        )}

        {submitError && (
          <div role="alert" className="wizard-error-banner" data-testid="wc-error">
            {submitError}
          </div>
        )}

        <div className="wizard-submit-row">
          <button type="button" data-testid="wc-submit" onClick={onSubmit} disabled={!canSubmit}>
            {state.inFlight ? 'Creating account…' : 'Create account & continue'}
          </button>
          <button
            type="button"
            className="wizard-btn-secondary"
            onClick={() => dispatch({ type: 'RETURN_TO_CREDENTIALS' })}
            disabled={state.inFlight}
          >
            Back to sign in
          </button>
        </div>
      </form>
    </div>
  );
}
