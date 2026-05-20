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
import { validateGrid } from './validators';
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
  const [identifier, setIdentifierLocal] = useState(state.identifier);
  const [grid, setGridLocal] = useState(state.grid);
  const [gridError, setGridError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<WizardError | null>(null);

  // ── Field handlers ─────────────────────────────────────────────────────

  const handleIdentifierChange = useCallback((value: string) => {
    setIdentifierLocal(value);
    dispatch({ type: 'SET_OFFLINE_FIELD', field: 'identifier', value });
    setSubmitError(null);
  }, [dispatch]);

  const handleGridChange = useCallback((value: string) => {
    setGridLocal(value);
    dispatch({ type: 'SET_OFFLINE_FIELD', field: 'grid', value });
    // Clear on change; re-validate on blur.
    setGridError(null);
    setSubmitError(null);
  }, [dispatch]);

  const handleGridBlur = useCallback(() => {
    setGridError(validateGrid(grid));
  }, [grid]);

  // ── Submit gate ─────────────────────────────────────────────────────────
  // Both fields are optional; the only block is a non-empty but invalid grid.
  const currentGridError = validateGrid(grid);
  const canSubmit = !currentGridError && !state.inFlight;

  // ── Submit handler ─────────────────────────────────────────────────────

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitError(null);
    dispatch({ type: 'SUBMIT_BEGIN' });
    try {
      await invoke('wizard_persist_offline', {
        identifier,
        grid,
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
        Both fields are optional — you can configure identity later via{' '}
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

        {/* Grid (optional, 4-char broadcast precision per Principle 7) */}
        <div className="wizard-field">
          <label htmlFor="w-grid">Grid locator (optional)</label>
          <input
            id="w-grid"
            type="text"
            placeholder="e.g. EM75"
            value={grid}
            onChange={e => handleGridChange(e.target.value)}
            onBlur={handleGridBlur}
            disabled={state.inFlight}
            aria-describedby={gridError ? 'grid-error' : 'grid-hint'}
          />
          {gridError && grid ? (
            <span id="grid-error" role="alert" className="wizard-field-error">
              {gridError}
            </span>
          ) : (
            <span id="grid-hint" className="wizard-field-hint">
              4-character Maidenhead locator (e.g. EM75). Tuxlink broadcasts at
              4-char precision by default (configurable in Settings).
            </span>
          )}
        </div>

        {/* Error banner (Busy is silent — no banner rendered) */}
        {errorText && (
          <div role="alert" className="wizard-error-banner">
            {errorText}
          </div>
        )}

        {/* Footer copy per spec §5.4 */}
        <p className="wizard-footer-copy">
          All fields optional. Tuxlink works fully offline — you can configure
          identity later via <strong>Tools → Settings</strong>.
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
