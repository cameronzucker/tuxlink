// Step2Credentials.tsx — wizard cluster Task 3.3 / tuxlink-1r5
// Spec: §3.3 (Step 2-CMS), §3.5 (error UX), §3.7 (shell-open for Register link)
//
// Form with callsign / password (with show/hide toggle) / grid / MBO address.
// Two submit paths:
//   Continue → wizard_persist_cms → SUBMIT_CREDENTIALS_SUCCESS(skipCmsVerify=false) → cms_verify step
//   Save-and-skip → wizard_persist_cms → SUBMIT_CREDENTIALS_SUCCESS(skipCmsVerify=true) → complete
//
// Password is cleared from WizardState on success (spec §3.1 invariant 1).
// Buttons disabled during inFlight (spec §3.1).
// Register link opens in system browser via tauri-plugin-shell (spec §3.7).

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { useWizard } from './wizardContext';
import { validateCallsign, validatePassword } from './validators';
import type { WizardError } from './types';

// Winlink account registration URL — opened in system browser, never in webview (spec §3.7).
const WINLINK_REGISTER_URL = 'https://www.winlink.org/user/register';

// Auto-fill the MBO address when the callsign changes, BUT only when:
// - The MBO field is empty, OR
// - The MBO field currently matches the PREVIOUS auto-filled value (not operator-customized).
// This prevents clobbering an operator's custom MBO setting.
function autoFillMbo(
  newCallsign: string,
  currentMbo: string,
  prevAutoFilled: string
): string {
  const auto = newCallsign ? `${newCallsign.toUpperCase()}@winlink.org` : '';
  if (!currentMbo || currentMbo === prevAutoFilled) {
    return auto;
  }
  return currentMbo;
}

function errorMessage(err: WizardError): string {
  switch (err.kind) {
    case 'Unavailable':
      return (
        "Tuxlink couldn't find a secret-service keyring on your system. " +
        "Tuxlink uses the OS keyring to store your Winlink CMS password securely " +
        "(instead of saving it to a config file). Install and start one " +
        "(e.g., `sudo apt install gnome-keyring`) and re-run the wizard."
      );
    case 'Locked':
      return (
        "Your keyring is currently locked. Unlock it via your desktop's keyring " +
        "tool (Seahorse on GNOME, kwallet manager on KDE) and click Retry."
      );
    case 'PermissionDenied': {
      const hint = err.detail?.platform_hint ?? 'linux';
      if (hint === 'macos') {
        return (
          "macOS Keychain requires you to authorize tuxlink to store your password. " +
          "A system dialog should have appeared; if you clicked Deny, click Retry " +
          "and authorize when prompted."
        );
      }
      if (hint === 'windows') {
        return (
          "Windows CredentialManager refused the write. Check that no group policy " +
          "is blocking generic credential storage, or report the issue at " +
          "github.com/cameronzucker/tuxlink/issues."
        );
      }
      return (
        "The keyring daemon refused the write. This is unusual on Linux; check your " +
        "distro's keyring permission settings or report the issue at " +
        "github.com/cameronzucker/tuxlink/issues."
      );
    }
    case 'ConfigWrite':
      return (
        `Tuxlink wrote your password to the keyring but couldn't save the config file ` +
        `(disk full? permissions?). Tuxlink has attempted to remove the keyring entry. ` +
        `Details: ${err.detail?.detail ?? 'unknown'}`
      );
    case 'ConfigWriteAndRollbackFailed':
      return (
        `Tuxlink couldn't save the config file AND the attempt to remove the keyring ` +
        `entry also failed. Run \`secret-tool delete service tuxlink account <callsign>\` ` +
        `manually before retrying. Config error: ${err.detail?.config_error ?? ''}; ` +
        `Rollback error: ${err.detail?.rollback_error ?? ''}`
      );
    case 'Busy':
      return ''; // silent per spec §3.5 — ErrBusy shows no user-visible message
    case 'InvalidInput':
      return `The callsign field contains characters tuxlink can't handle (non-ASCII, ` +
        `zero-width, or homoglyph). Re-type using only A-Z, 0-9, and /.`;
    case 'Other':
      return (
        `An unexpected error occurred while saving credentials. ` +
        `Details: ${err.detail?.detail ?? 'unknown'}. ` +
        `If this looks like a tuxlink bug, please report at github.com/cameronzucker/tuxlink/issues.`
      );
    default:
      return 'An unexpected error occurred. Please try again.';
  }
}

export function Step2Credentials() {
  const { state, dispatch } = useWizard();

  // Local form state — not hoisted to wizard reducer (password security).
  // The reducer's state.callsign / state.mboAddress are the persisted fields; the
  // password field is local-only until submit. Grid is no longer collected here —
  // it moved to the dedicated Location step (tuxlink-9xy1), which owns GPS source
  // detection + manual grid entry for the whole wizard.
  const [callsign, setCallsignLocal] = useState(state.callsign);
  const [password, setPassword] = useState(state.password);
  const [mboAddress, setMboAddress] = useState(state.mboAddress);

  // Tracks the last auto-filled MBO value so we know when it's still auto-filled.
  const [lastAutoMbo, setLastAutoMbo] = useState<string>(state.mboAddress);

  const [showPassword, setShowPassword] = useState(false);
  const [submitError, setSubmitError] = useState<WizardError | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string | null>>({});

  // ── Field handlers ─────────────────────────────────────────────────────

  const handleCallsignChange = useCallback((value: string) => {
    setCallsignLocal(value);
    dispatch({ type: 'SET_CREDENTIALS_FIELD', field: 'callsign', value });
    // Update the MBO auto-fill if not operator-customized.
    const newMbo = autoFillMbo(value, mboAddress, lastAutoMbo);
    if (newMbo !== mboAddress) {
      setMboAddress(newMbo);
      setLastAutoMbo(newMbo);
      dispatch({ type: 'SET_CREDENTIALS_FIELD', field: 'mboAddress', value: newMbo });
    }
    // Clear callsign field error on change.
    setFieldErrors(prev => ({ ...prev, callsign: null }));
    setSubmitError(null);
  }, [dispatch, mboAddress, lastAutoMbo]);

  const handleCallsignBlur = useCallback(() => {
    const err = validateCallsign(callsign);
    setFieldErrors(prev => ({ ...prev, callsign: err }));
  }, [callsign]);

  const handlePasswordChange = useCallback((value: string) => {
    setPassword(value);
    dispatch({ type: 'SET_CREDENTIALS_FIELD', field: 'password', value });
    setSubmitError(null);
  }, [dispatch]);

  const handleMboChange = useCallback((value: string) => {
    setMboAddress(value);
    // If the operator is typing something other than the auto-filled value,
    // it counts as customized — don't overwrite on next callsign change.
    dispatch({ type: 'SET_CREDENTIALS_FIELD', field: 'mboAddress', value });
  }, [dispatch]);

  // ── Validation gate for submit buttons ────────────────────────────────

  const callsignError = validateCallsign(callsign);
  const passwordError = validatePassword(password);
  const canSubmit = !callsignError && !passwordError && !state.inFlight;

  // ── Submit handler ────────────────────────────────────────────────────

  async function handleSubmit(skipCmsVerify: boolean) {
    if (!canSubmit) return;
    setSubmitError(null);
    dispatch({ type: 'SUBMIT_BEGIN' });
    try {
      await invoke('wizard_persist_cms', {
        rawCallsign: callsign,
        password,
        // Grid is set later in the Location step (via config_set_grid); the wizard
        // no longer collects it here (tuxlink-9xy1). Pass empty to satisfy the command.
        grid: '',
        mboAddress,
      });
      // Clear local password immediately after successful invoke.
      setPassword('');
      dispatch({ type: 'SUBMIT_CREDENTIALS_SUCCESS', skipCmsVerify });
    } catch (err) {
      const wizErr = err as WizardError;
      if (wizErr.kind === 'Busy') {
        // Busy = silent (UI debounce should have prevented this)
        dispatch({ type: 'SUBMIT_FAILURE', error: wizErr });
        return;
      }
      setSubmitError(wizErr);
      dispatch({ type: 'SUBMIT_FAILURE', error: wizErr });
    }
  }

  // ── Register link ─────────────────────────────────────────────────────

  function handleRegisterClick(e: React.MouseEvent<HTMLAnchorElement>) {
    e.preventDefault();
    shellOpen(WINLINK_REGISTER_URL).catch(console.error);
  }

  // ── Render ────────────────────────────────────────────────────────────

  const errorText = submitError ? errorMessage(submitError) : null;
  const showRetry = submitError?.kind === 'Locked';

  return (
    <div className="wizard-step wizard-step-credentials">
      <h1>Your Winlink CMS credentials</h1>
      <p>
        Enter the callsign and password for your{' '}
        <a href={WINLINK_REGISTER_URL} onClick={handleRegisterClick} role="link" aria-label="Register a Winlink account">
          Winlink account
        </a>
        . Don't have one? Click "Register" above to create one first.
      </p>

      <form noValidate onSubmit={e => e.preventDefault()}>
        {/* Callsign field */}
        <div className="wizard-field">
          <label htmlFor="w-callsign">Callsign *</label>
          <input
            id="w-callsign"
            type="text"
            autoCapitalize="characters"
            autoComplete="username"
            value={callsign}
            onChange={e => handleCallsignChange(e.target.value)}
            onBlur={handleCallsignBlur}
            disabled={state.inFlight}
            aria-describedby={fieldErrors.callsign ? 'callsign-error' : undefined}
          />
          {fieldErrors.callsign && (
            <span id="callsign-error" role="alert" className="wizard-field-error">
              {fieldErrors.callsign}
            </span>
          )}
        </div>

        {/* Password field with show/hide toggle */}
        <div className="wizard-field">
          <label htmlFor="w-password">CMS password *</label>
          <div className="wizard-password-row">
            <input
              id="w-password"
              type={showPassword ? 'text' : 'password'}
              autoComplete="current-password"
              value={password}
              onChange={e => handlePasswordChange(e.target.value)}
              disabled={state.inFlight}
            />
            <button
              type="button"
              onClick={() => setShowPassword(v => !v)}
              aria-label={showPassword ? 'Conceal password field' : 'Reveal password field'}
            >
              {showPassword ? 'Hide' : 'Show'}
            </button>
          </div>
        </div>

        {/* Grid moved to the dedicated Location step (tuxlink-9xy1). */}

        {/* MBO address (optional, auto-fills from callsign) */}
        <div className="wizard-field">
          <label htmlFor="w-mbo">MBO address (optional)</label>
          <input
            id="w-mbo"
            type="text"
            placeholder="e.g. W4PHS@winlink.org"
            value={mboAddress}
            onChange={e => handleMboChange(e.target.value)}
            disabled={state.inFlight}
          />
        </div>

        {/* Error banner */}
        {errorText && (
          <div role="alert" className="wizard-error-banner">
            {errorText}
            {showRetry && (
              <button
                type="button"
                onClick={() => handleSubmit(false)}
                disabled={state.inFlight}
              >
                Retry
              </button>
            )}
          </div>
        )}

        {/* Submit buttons */}
        <div className="wizard-submit-row">
          <button
            type="button"
            onClick={() => handleSubmit(false)}
            disabled={!canSubmit}
          >
            Continue
          </button>
          <button
            type="button"
            onClick={() => handleSubmit(true)}
            disabled={!canSubmit}
            className="wizard-btn-secondary"
          >
            Save credentials and skip verification
          </button>
        </div>
      </form>
    </div>
  );
}
