// CredentialFields.tsx — shared callsign + password input pair (tuxlink-vfb3).
//
// Extracted from Step2Credentials so the wizard's Step 2 and the Settings
// "Winlink Account" re-enter form render identical fields. Controlled component:
// the parent owns `callsign` / `password` (the wizard keeps callsign in its
// reducer for MBO auto-fill + password in local state; Settings owns its own);
// only the show/hide MASK toggle is internal display state.
//
// MBO is intentionally NOT here (callsign + password only) — it stays in
// Step2Credentials, which owns the callsign→MBO auto-fill logic.
//
// `idPrefix` namespaces the input ids (default 'w' → 'w-callsign' / 'w-password'
// to keep the wizard's existing ids + label associations stable) so a second
// instance on the same surface never collides.

import { useState } from 'react';
import { cmsPasswordTruncationNotice } from './validators';

export interface CredentialFieldsProps {
  callsign: string;
  password: string;
  onCallsignChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  /** Fired when the callsign input loses focus (wizard uses it to validate). */
  onCallsignBlur?: () => void;
  /** Field-level callsign error; rendered as an inline alert when truthy. */
  callsignError?: string | null;
  /** Disable both inputs + the mask toggle (e.g. during an in-flight submit). */
  disabled?: boolean;
  /** Namespaces input ids + the error id. Default 'w' (wizard back-compat). */
  idPrefix?: string;
  callsignLabel?: string;
  passwordLabel?: string;
  /** autoComplete hint for the password input ('current-password' default;
   *  'new-password' makes no sense here since this is the existing credential). */
  passwordAutoComplete?: string;
}

export function CredentialFields({
  callsign,
  password,
  onCallsignChange,
  onPasswordChange,
  onCallsignBlur,
  callsignError,
  disabled = false,
  idPrefix = 'w',
  callsignLabel = 'Callsign *',
  passwordLabel = 'CMS password *',
  passwordAutoComplete = 'current-password',
}: CredentialFieldsProps) {
  const [showPassword, setShowPassword] = useState(false);

  const callsignId = `${idPrefix}-callsign`;
  const passwordId = `${idPrefix}-password`;
  const errorId = `${idPrefix}-callsign-error`;

  // Non-blocking advisory: the CMS truncates a password to its first 12 chars.
  const truncationNotice = cmsPasswordTruncationNotice(password);

  return (
    <>
      {/* Callsign field */}
      <div className="wizard-field">
        <label htmlFor={callsignId}>{callsignLabel}</label>
        <input
          id={callsignId}
          type="text"
          autoCapitalize="characters"
          autoComplete="username"
          value={callsign}
          onChange={(e) => onCallsignChange(e.target.value)}
          onBlur={onCallsignBlur}
          disabled={disabled}
          aria-describedby={callsignError ? errorId : undefined}
        />
        {callsignError && (
          <span id={errorId} role="alert" className="wizard-field-error">
            {callsignError}
          </span>
        )}
      </div>

      {/* Password field with show/hide toggle */}
      <div className="wizard-field">
        <label htmlFor={passwordId}>{passwordLabel}</label>
        <div className="wizard-password-row">
          <input
            id={passwordId}
            type={showPassword ? 'text' : 'password'}
            autoComplete={passwordAutoComplete}
            value={password}
            onChange={(e) => onPasswordChange(e.target.value)}
            disabled={disabled}
          />
          <button
            type="button"
            onClick={() => setShowPassword((v) => !v)}
            disabled={disabled}
            aria-label={showPassword ? 'Conceal password field' : 'Reveal password field'}
          >
            {showPassword ? 'Hide' : 'Show'}
          </button>
        </div>
        {truncationNotice && (
          <span className="wizard-field-notice" data-testid={`${idPrefix}-password-truncation-notice`}>
            {truncationNotice}
          </span>
        )}
      </div>
    </>
  );
}
