// Step2OfflineIdentity.tsx — wizard cluster Task 11.5 / tuxlink-d76
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.3 + §5.4
//
// Offline path: station identifier (free-form, optional) + grid (4-char, optional).
// "All fields optional" — both fields blank is a valid submit (offline mode works
//   without any identity configured; operator can set later via Tools → Settings).
//
// Submit path:
//   "Continue offline" → invoke wizard_persist_offline(identifier, grid) →
//     SUBMIT_OFFLINE_SUCCESS → step = 'complete'
//
// NO keyring access. NO password. NO callsign requirement. (Part 97: no transmission.)
// Single-flight mutex guard in Rust prevents multi-window double-dispatch (spec §3.7).
// Busy error is silent per spec §3.5 — ErrBusy shows no user-visible message.

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useWizard } from './wizardContext';
import type { WizardError } from './types';

function errorMessage(err: WizardError): string | null {
  switch (err.kind) {
    case 'Busy':
      // Silent per spec §3.5 — ErrBusy needs no operator-visible message.
      return null;
    case 'ConfigWrite':
      return (
        `Tuxlink couldn't save the config file ` +
        `(disk full? permissions?). ` +
        `Details: ${err.detail?.detail ?? 'unknown'}`
      );
    case 'ConfigWriteAndRollbackFailed':
      return (
        `Tuxlink couldn't save the config file. ` +
        `Details: ${err.detail?.config_error ?? ''}`
      );
    case 'InvalidInput':
      return `One or more fields contain characters tuxlink can't handle. Re-check your input.`;
    case 'Other':
      return (
        `An unexpected error occurred. ` +
        `Details: ${err.detail?.detail ?? 'unknown'}. ` +
        `If this looks like a tuxlink bug, please report at github.com/cameronzucker/tuxlink/issues.`
      );
    default:
      return 'An unexpected error occurred. Please try again.';
  }
}

export function Step2OfflineIdentity() {
  const { state, dispatch } = useWizard();

  // Local form state — synced to reducer via SET_OFFLINE_FIELD.
  // Grid moved to the dedicated Location step (tuxlink-9xy1) — this step now only
  // collects the optional station identifier.
  const [identifier, setIdentifierLocal] = useState(state.identifier);
  const [submitError, setSubmitError] = useState<WizardError | null>(null);

  // ── Field handlers ─────────────────────────────────────────────────────

  const handleIdentifierChange = useCallback((value: string) => {
    setIdentifierLocal(value);
    dispatch({ type: 'SET_OFFLINE_FIELD', field: 'identifier', value });
    setSubmitError(null);
  }, [dispatch]);

  // ── Submit gate ─────────────────────────────────────────────────────────
  // Identifier is optional and free-form; nothing here blocks submit except an
  // in-flight request. (Grid + its validation moved to the Location step.)
  const canSubmit = !state.inFlight;

  // ── Submit handler ─────────────────────────────────────────────────────

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitError(null);
    dispatch({ type: 'SUBMIT_BEGIN' });
    try {
      await invoke('wizard_persist_offline', {
        identifier,
        // Grid is set later in the Location step (via config_set_grid); pass empty
        // to satisfy the command signature (tuxlink-9xy1).
        grid: '',
      });
      dispatch({ type: 'SUBMIT_OFFLINE_SUCCESS' });
    } catch (err) {
      const wizErr = err as WizardError;
      if (wizErr.kind === 'Busy') {
        // Busy = silent — UI inFlight flag is the primary guard; mutex is the backstop.
        dispatch({ type: 'SUBMIT_FAILURE', error: wizErr });
        return;
      }
      setSubmitError(wizErr);
      dispatch({ type: 'SUBMIT_FAILURE', error: wizErr });
    }
  }

  // ── Render ────────────────────────────────────────────────────────────

  const errorText = submitError ? errorMessage(submitError) : null;

  return (
    <div className="wizard-step wizard-step-offline-identity">
      <h1>Offline station identity</h1>
      <p>
        Identify this station for radio-network sessions.
        It's optional — you can configure identity later via{' '}
        <strong>Tools → Settings</strong>.
      </p>

      <form noValidate onSubmit={e => e.preventDefault()}>
        {/* Station identifier (free-form, optional) */}
        <div className="wizard-field">
          <label htmlFor="w-identifier">Station identifier (optional)</label>
          <input
            id="w-identifier"
            type="text"
            placeholder="e.g. EOC-1, ARES-NET, W4PHS"
            value={identifier}
            onChange={e => handleIdentifierChange(e.target.value)}
            disabled={state.inFlight}
            aria-describedby="identifier-hint"
          />
          <span id="identifier-hint" className="wizard-field-hint">
            Free-form — accepts callsigns, tactical addresses, or any identifier.
          </span>
        </div>

        {/* Grid moved to the dedicated Location step (tuxlink-9xy1). */}

        {/* Error banner (Busy is silent — no banner rendered) */}
        {errorText && (
          <div role="alert" className="wizard-error-banner">
            {errorText}
          </div>
        )}

        {/* Footer copy per spec §5.4 */}
        <p className="wizard-footer-copy">
          Optional. Tuxlink works fully offline — you can configure identity later
          via <strong>Tools → Settings</strong>. You'll set your location next.
        </p>

        {/* Single submit button */}
        <div className="wizard-submit-row">
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!canSubmit}
          >
            Continue offline
          </button>
        </div>
      </form>
    </div>
  );
}
