/**
 * IdentitySwitcher — the dashboard callsign chip with an inline identity
 * switcher dropdown (Phase 7, tuxlink-noa0).
 *
 * Closed: reproduces the DashboardRibbon `.dash-callsign-row` (callsign text as
 * a reset-to-text trigger button + the SSID `<select>`), so the footprint is
 * unchanged. Clicking the callsign text opens an in-flow dropdown anchored
 * below the chip; the SSID select does NOT open it.
 *
 * Open: a `role="listbox"` listing each FULL identity with its tacticals nested
 * directly beneath (the wire list is FLAT — tacticals are derived by matching
 * `tactical.parent === full.callsign`). Esc / click-outside close.
 *
 * Switching reconciliation: every identity switch AUTHENTICATES the parent FULL
 * via a credential — there is no switch-without-auth path on the backend. So:
 *   - clicking the row that is ALREADY active → close the dropdown (no-op);
 *   - clicking ANY other row → reveal an inline unlock field within the list;
 *   - on submit → `onSwitch({ callsign: <parent FULL>, credential, tacticalLabel })`.
 * For a FULL row the callsign is itself and `tacticalLabel` is null; for a
 * tactical row the callsign is its parent FULL and `tacticalLabel` is the
 * tactical's label (authenticate the parent, present as the tactical).
 *
 * The credential lives ONLY in component state — never in the list DTO, never
 * logged. This is access-control credential entry, NOT a RADIO-1 TX-consent
 * modal; no consent modal is added here.
 *
 * Inline-edit structure (open/close + Esc-cancel + catch-and-stay-in-edit)
 * mirrors GridEdit.
 */

import { useEffect, useRef, useState } from 'react';
import type {
  ActiveIdentityDto,
  CmsBadge,
  FullIdentityDto,
  IdentityListDto,
  TacticalIdentityDto,
} from './identityTypes';
import { parseIdentityError } from './identityTypes';
import { ssidOptions } from '../packet/packetConfig';
import './IdentitySwitcher.css';

export interface IdentitySwitcherProps {
  /** Active session for the closed-chip label; null pre-auth → fall back to
   *  `list.last_selected` / em-dash (never a stale call). */
  active: ActiveIdentityDto | null;
  /** Dropdown contents; null while loading → a placeholder row. */
  list: IdentityListDto | null;
  /** Effective AX.25 SSID (0..15) for the chip's SSID select. */
  ssid?: number;
  /** Persist a new SSID. When omitted, the SSID renders as a plain text span. */
  onSsidChange?: (n: number) => void;
  /** Authenticate + switch. The credential is the typed unlock value; the
   *  callsign is always the parent FULL; tacticalLabel is the presented
   *  tactical (or null for a FULL row). */
  onSwitch: (args: { callsign: string; credential: string; tacticalLabel: string | null }) => Promise<void>;
}

/** Identifies which row is currently revealing its inline unlock field. */
type UnlockTarget =
  | { kind: 'full'; callsign: string }
  | { kind: 'tactical'; parent: string; label: string };

function tacticalCmsBadge(badge: CmsBadge) {
  switch (badge) {
    case 'registered':
      return (
        <span
          className="cms-badge cms-badge--ok"
          data-testid="cms-badge-ok"
          title="CMS account verified — CMS modes available."
        >
          ✓ CMS
        </span>
      );
    case 'not_registered':
      return (
        <span
          className="cms-badge cms-badge--blocked"
          data-testid="cms-badge-blocked"
          title="CMS modes unavailable until this tactical's registration is verified."
        >
          ⊘ CMS
        </span>
      );
    case 'unknown':
    default:
      // Fail-closed: an unknown registration state is treated as blocked.
      return (
        <span
          className="cms-badge cms-badge--unknown"
          data-testid="cms-badge-unknown"
          title="CMS registration unverified — CMS modes unavailable until registration is verified."
        >
          ? CMS
        </span>
      );
  }
}

export function IdentitySwitcher({ active, list, ssid, onSsidChange, onSwitch }: IdentitySwitcherProps) {
  const [open, setOpen] = useState(false);
  const [unlockTarget, setUnlockTarget] = useState<UnlockTarget | null>(null);
  const [credential, setCredential] = useState('');
  const [error, setError] = useState<string | null>(null);
  const rowRef = useRef<HTMLDivElement>(null);

  // Closed-chip label: prefer the active session; pre-auth fall back to
  // last_selected, then an em-dash. Never a stale SSID-bound call.
  const primaryLabel = active?.address_as ?? list?.last_selected ?? '—';
  const parentIndicator = active?.is_tactical ? active.mycall : null;

  // Reset transient state whenever the dropdown closes.
  function closeDropdown() {
    setOpen(false);
    setUnlockTarget(null);
    setCredential('');
    setError(null);
  }

  // Esc (anywhere in the open dropdown) + click-outside close the dropdown.
  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      if (rowRef.current && !rowRef.current.contains(e.target as Node)) {
        closeDropdown();
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  // Is this row the currently-active identity? Re-selecting it is a no-op.
  function isActiveAddress(addressAs: string): boolean {
    return active != null && active.address_as === addressAs;
  }

  function revealUnlock(target: UnlockTarget) {
    setUnlockTarget(target);
    setCredential('');
    setError(null);
  }

  function cancelUnlock() {
    setUnlockTarget(null);
    setCredential('');
    setError(null);
  }

  function handleFullRowClick(full: FullIdentityDto) {
    if (isActiveAddress(full.callsign)) {
      closeDropdown();
      return;
    }
    revealUnlock({ kind: 'full', callsign: full.callsign });
  }

  function handleTacticalRowClick(t: TacticalIdentityDto) {
    if (isActiveAddress(t.label)) {
      closeDropdown();
      return;
    }
    revealUnlock({ kind: 'tactical', parent: t.parent, label: t.label });
  }

  function submitUnlock() {
    if (!unlockTarget) return;
    const callsign = unlockTarget.kind === 'full' ? unlockTarget.callsign : unlockTarget.parent;
    const tacticalLabel = unlockTarget.kind === 'full' ? null : unlockTarget.label;
    onSwitch({ callsign, credential, tacticalLabel })
      .then(() => {
        closeDropdown();
      })
      .catch((err: unknown) => {
        // Keep the field open + retain the typed value so the operator retries.
        setError(parseIdentityError(err));
      });
  }

  function handleUnlockKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      cancelUnlock();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      submitUnlock();
    }
  }

  // The label shown in the unlock form for the row being unlocked.
  const unlockLabel = unlockTarget
    ? unlockTarget.kind === 'full'
      ? unlockTarget.callsign
      : unlockTarget.label
    : '';

  // Does a given full/tactical row currently own the inline unlock form?
  function fullIsUnlocking(callsign: string): boolean {
    return unlockTarget?.kind === 'full' && unlockTarget.callsign === callsign;
  }
  function tacticalIsUnlocking(label: string): boolean {
    return unlockTarget?.kind === 'tactical' && unlockTarget.label === label;
  }

  function renderUnlockForm() {
    return (
      <div className="identity-unlock" data-testid="identity-unlock">
        <label htmlFor="identity-unlock-input">Unlock {unlockLabel}</label>
        <input
          id="identity-unlock-input"
          className="identity-unlock-input"
          data-testid="identity-unlock-input"
          type="password"
          autoFocus
          value={credential}
          onChange={(e) => {
            setCredential(e.target.value);
            setError(null);
          }}
          onKeyDown={handleUnlockKeyDown}
        />
        <div className="identity-unlock-actions">
          <button
            type="button"
            className="identity-unlock-submit"
            data-testid="identity-unlock-submit"
            onClick={submitUnlock}
          >
            Unlock
          </button>
        </div>
        {error && (
          <div className="identity-unlock-error" data-testid="identity-unlock-error" role="alert">
            {error}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      ref={rowRef}
      className="dash-value callsign dash-callsign-row identity-switcher-row"
      data-testid="ribbon-callsign"
    >
      <button
        type="button"
        className="identity-switcher-trigger"
        data-testid="identity-switcher-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? closeDropdown() : setOpen(true))}
      >
        <span className="dash-callsign-text" data-testid="ribbon-callsign-text">
          {primaryLabel}
        </span>
        {parentIndicator && (
          <span className="identity-active-parent" data-testid="identity-active-parent">
            ({parentIndicator})
          </span>
        )}
      </button>

      {onSsidChange ? (
        <select
          className="dash-callsign-select dash-ssid-select"
          data-testid="ribbon-ssid-select"
          aria-label="AX.25 SSID"
          title="Click to switch AX.25 SSID"
          value={ssid ?? 0}
          // Sibling placement already keeps this out of the trigger; stop the
          // mousedown/click from bubbling to any ancestor open handler too.
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => onSsidChange(Number(e.target.value))}
        >
          {ssidOptions().map((n) => (
            <option key={n} value={n}>{`-${n}`}</option>
          ))}
        </select>
      ) : null}

      {open && (
        <div
          className="identity-switcher-list"
          data-testid="identity-switcher-list"
          role="listbox"
          tabIndex={-1}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              closeDropdown();
            }
          }}
        >
          {list == null ? (
            <div className="identity-list-loading" data-testid="identity-list-loading">
              Loading identities…
            </div>
          ) : (
            list.full.map((full) => {
              const tacticals = list.tactical.filter((t) => t.parent === full.callsign);
              const fullCurrent = list.last_selected === full.callsign;
              const fullLocked = full.needs_auth;
              return (
                <div key={`full-${full.callsign}`} className="identity-group">
                  <button
                    type="button"
                    role="option"
                    aria-selected={isActiveAddress(full.callsign)}
                    aria-current={fullCurrent ? 'true' : undefined}
                    aria-label={fullLocked ? `${full.callsign} (locked)` : full.callsign}
                    className="identity-row identity-row--full"
                    data-testid={`identity-row-full-${full.callsign}`}
                    onClick={() => handleFullRowClick(full)}
                  >
                    <span className="identity-row-label">
                      {full.callsign}
                      {full.label ? ` · ${full.label}` : ''}
                    </span>
                    {fullLocked && (
                      <span className="identity-lock" aria-hidden="true">
                        🔒
                      </span>
                    )}
                  </button>
                  {fullIsUnlocking(full.callsign) && renderUnlockForm()}

                  {tacticals.map((t) => {
                    const tCurrent = list.last_selected === t.label;
                    return (
                      <div key={`tactical-${t.label}`} className="identity-group">
                        <button
                          type="button"
                          role="option"
                          aria-selected={isActiveAddress(t.label)}
                          aria-current={tCurrent ? 'true' : undefined}
                          aria-label={t.label}
                          className="identity-row identity-row--tactical"
                          data-testid={`identity-row-tactical-${t.label}`}
                          onClick={() => handleTacticalRowClick(t)}
                        >
                          <span className="identity-row-label">{t.label}</span>
                          {tacticalCmsBadge(t.cms_badge)}
                        </button>
                        {tacticalIsUnlocking(t.label) && renderUnlockForm()}
                      </div>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
