// CmsAccountDelete.tsx — delete a Winlink CMS account from Tuxlink (tuxlink-vfb3
// sub-project 3). DESTRUCTIVE: removes the account on the Winlink CMS server and
// deletes the local keyring credential on success.
//
// Safety UX (not a build gate — the op is fully wired): a typed-confirmation gate.
// The Delete button stays disabled until the operator types the exact callsign,
// matching the GitHub "type the repo name to delete" convention. account_remove is
// privilege-gated server-side (its live metadata is 403); if the configured access
// key is not authorized, the command returns a rejection and this surfaces it
// plainly rather than pretending the account was removed.
//
// Gated on TUXLINK_WINLINK_ACCESS_CODE (cms_password_change_available): the open
// build ships no key, so the whole account API reports unavailable and this renders
// nothing — consistent with CmsPasswordChange.
//
// RADIO-1: internet HTTPS to the account API, not a transmission. The account
// mutation is real, so live exercise is operator-run.

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface CmsAccountDeleteProps {
  /** The account callsign to delete (the active identity). */
  callsign: string;
  /** Called after a confirmed server-side removal so the shell can react (the
   *  identity is now invalid). */
  onDeleted?: () => void;
}

interface BackendError {
  kind?: string;
  code?: string;
  message?: string;
}

/** Map an AccountApiError to operator-facing copy. */
function deleteError(be: BackendError): string {
  switch (be?.kind) {
    case 'InvalidKey':
      return 'This Tuxlink build is not authorized to delete Winlink accounts. Remove the account at winlink.org instead.';
    case 'Rejected':
      // account/remove is privilege-gated (403); a rejection most likely means the
      // configured key cannot invoke it. Surface the server message verbatim.
      return be.message
        ? `Winlink refused the deletion: ${be.message}`
        : 'Winlink refused the deletion. The access key may not be authorized to remove accounts.';
    case 'KeyringDesync':
      return 'The account was deleted on Winlink, but clearing the local saved password failed. The stored credential is now stale; clear it from Settings.';
    case 'UnknownOutcome':
      return 'The request timed out before the result could be confirmed. The account may or may not have been deleted — check at winlink.org before retrying.';
    case 'Network':
      return 'Could not reach the Winlink account service. Check your connection and try again.';
    case 'NotConfigured':
      return 'Account deletion is unavailable on this build.';
    default:
      return be?.message ?? 'Account deletion failed.';
  }
}

export function CmsAccountDelete({ callsign, onDeleted }: CmsAccountDeleteProps) {
  const [available, setAvailable] = useState(false);
  const [confirmText, setConfirmText] = useState('');
  const [inFlight, setInFlight] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleted, setDeleted] = useState(false);

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

  // Case-insensitive exact match of the typed callsign (matches the backend's
  // uppercasing normalization).
  const confirmed = confirmText.trim().toUpperCase() === callsign.trim().toUpperCase();
  const canDelete = confirmed && !inFlight && !deleted;

  const onDelete = useCallback(async () => {
    if (!canDelete) return;
    setError(null);
    setInFlight(true);
    try {
      await invoke('cms_account_remove', { rawCallsign: callsign });
      setConfirmText('');
      setDeleted(true);
      onDeleted?.();
    } catch (e) {
      setError(deleteError(e as BackendError));
    } finally {
      setInFlight(false);
    }
  }, [canDelete, callsign, onDeleted]);

  if (!available) return null;

  if (deleted) {
    return (
      <section className="tux-account-danger" data-testid="account-delete">
        <h2>Delete Winlink account</h2>
        <div role="status" className="wizard-success" data-testid="account-delete-success">
          {callsign} was deleted from Winlink and its saved password was removed from this
          computer. Add or switch to another identity to continue.
        </div>
      </section>
    );
  }

  return (
    <section className="tux-account-danger" data-testid="account-delete">
      <h2>Delete Winlink account</h2>
      <p className="wizard-hint">
        Permanently delete <strong>{callsign}</strong> from the Winlink CMS and remove its
        saved password from this computer. This cannot be undone. Messages already sent are
        unaffected; the callsign can be re-registered later.
      </p>

      <div className="wizard-field">
        <label htmlFor="account-delete-confirm">
          Type <strong>{callsign}</strong> to confirm
        </label>
        <input
          id="account-delete-confirm"
          data-testid="account-delete-confirm"
          type="text"
          autoCapitalize="characters"
          autoComplete="off"
          value={confirmText}
          onChange={(e) => {
            setConfirmText(e.target.value);
            setError(null);
          }}
          disabled={inFlight}
        />
      </div>

      {error && (
        <div role="alert" className="wizard-field-error" data-testid="account-delete-error">
          {error}
        </div>
      )}

      <button
        type="button"
        className="tux-danger-button"
        data-testid="account-delete-submit"
        disabled={!canDelete}
        onClick={onDelete}
      >
        {inFlight ? 'Deleting…' : `Delete ${callsign} permanently`}
      </button>
    </section>
  );
}
