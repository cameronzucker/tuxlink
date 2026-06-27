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

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import type {
  ActiveIdentityDto,
  CmsBadge,
  FullIdentityDto,
  IdentityListDto,
  TacticalIdentityDto,
} from './identityTypes';
import { parseIdentityError } from './identityTypes';
import './IdentitySwitcher.css';

export interface IdentitySwitcherProps {
  /** Active session for the closed-chip label; null pre-auth → fall back to
   *  `list.last_selected` / em-dash (never a stale call). */
  active: ActiveIdentityDto | null;
  /** Dropdown contents; null while loading → a placeholder row. */
  list: IdentityListDto | null;
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

export function IdentitySwitcher({ active, list, onSwitch }: IdentitySwitcherProps) {
  const [open, setOpen] = useState(false);
  const [unlockTarget, setUnlockTarget] = useState<UnlockTarget | null>(null);
  const [credential, setCredential] = useState('');
  const [error, setError] = useState<string | null>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  // tuxlink-ru32: the dropdown is portaled to <body> with position:fixed, so it
  // escapes the ribbon's stacking context (was rendered under z-index-higher
  // shell regions). `listRef` lets click-outside still recognise clicks inside
  // the portaled panel; `coords` anchors it to the chip.
  const listRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<{ top: number; left: number; width: number } | null>(null);

  // Forgot-password recovery (tuxlink-vfb3 sub-project 2). The "Forgot password?"
  // affordance in the unlock form asks the CMS to email the account password to the
  // recovery address on file (account_send_recovery). Gated on the account-API key
  // (cms_password_change_available): without it the affordance is hidden. Recovering
  // a FULL account's password; tacticals ride their parent FULL.
  const [recoveryAvailable, setRecoveryAvailable] = useState(false);
  const [recovering, setRecovering] = useState(false);
  const [recoveryMsg, setRecoveryMsg] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => {
    let active = true;
    invoke<boolean>('cms_password_change_available')
      .then((v) => {
        if (active) setRecoveryAvailable(Boolean(v));
      })
      .catch(() => {
        if (active) setRecoveryAvailable(false);
      });
    return () => {
      active = false;
    };
  }, []);

  // Closed-chip label: prefer the active session; pre-auth fall back to
  // last_selected, then an em-dash. Never a stale SSID-bound call.
  const primaryLabel = active?.address_as ?? list?.last_selected ?? '—';
  const parentIndicator = active?.is_tactical ? active.mycall : null;

  // Locked = no identity authenticated this launch, but one IS configured. Auth
  // is in-memory and re-acquired per launch (auto-auth at bootstrap, else manual
  // unlock); when auto-auth cannot read the stored credential the active slot
  // stays empty and every transmit/egress fails closed. The closed chip otherwise
  // shows `last_selected` identically whether authenticated or not, so the
  // operator gets no at-a-glance signal that they are not transmit-ready. Surface
  // it here (the open dropdown already flags per-identity locks). An empty store
  // (em-dash, nothing to authenticate) is NOT "locked".
  const locked = active == null && list?.last_selected != null;

  // Reset transient state whenever the dropdown closes.
  function closeDropdown() {
    setOpen(false);
    setUnlockTarget(null);
    setCredential('');
    setError(null);
    setRecoveryMsg(null);
    setRecovering(false);
  }

  // Measure the chip and anchor the portaled panel just below it. Recompute on
  // open and on resize so the fixed-position panel tracks the chip.
  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    function measure() {
      const r = rowRef.current?.getBoundingClientRect();
      if (r) setCoords({ top: r.bottom + 6, left: r.left, width: r.width });
    }
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [open]);

  // Esc (anywhere in the open dropdown) + click-outside close the dropdown. The
  // panel is portaled out of `rowRef`, so a click inside it is "outside" the row
  // — check `listRef` too or interacting with the dropdown would close it.
  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      const target = e.target as Node;
      const inRow = rowRef.current?.contains(target);
      const inList = listRef.current?.contains(target);
      if (!inRow && !inList) {
        closeDropdown();
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  // Case-insensitive callsign/label compare, mirroring the backend auth contract
  // (`authenticate` + the needs_auth projection both fold ASCII case), so an
  // active "w1abc" is recognized against a stored "W1ABC" (MINOR 1).
  const eqCI = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

  // Is this FULL row the currently-active identity? (Only when the active
  // session presents AS a FULL, not as a tactical riding under it.)
  function isActiveFull(callsign: string): boolean {
    return active != null && !active.is_tactical && eqCI(active.address_as, callsign);
  }

  // Is this tactical row the active identity? A tactical label is NOT globally
  // unique (the same label may exist under two FULLs), so disambiguate by BOTH
  // the label AND the parent FULL (= the active session's mycall) — IMPORTANT 3.
  function isActiveTactical(t: TacticalIdentityDto): boolean {
    return (
      active != null &&
      active.is_tactical &&
      eqCI(active.address_as, t.label) &&
      eqCI(active.mycall, t.parent)
    );
  }

  function revealUnlock(target: UnlockTarget) {
    setUnlockTarget(target);
    setCredential('');
    setError(null);
    setRecoveryMsg(null);
  }

  function cancelUnlock() {
    setUnlockTarget(null);
    setCredential('');
    setError(null);
    setRecoveryMsg(null);
  }

  // "Forgot password?" — ask the CMS to email the account password to its recovery
  // address. The account is the FULL (a tactical rides its parent FULL).
  function sendRecovery() {
    if (!unlockTarget || recovering) return;
    const callsign = unlockTarget.kind === 'full' ? unlockTarget.callsign : unlockTarget.parent;
    setRecovering(true);
    setRecoveryMsg(null);
    invoke('cms_account_send_recovery', { rawCallsign: callsign })
      .then(() => {
        setRecoveryMsg({
          ok: true,
          text: `Your password was emailed to the recovery address on file for ${callsign}.`,
        });
      })
      .catch((e: { kind?: string; message?: string }) => {
        // The server errors when no recovery address is on file — surface its message
        // and point the user at where to set one.
        const text =
          e?.kind === 'Rejected'
            ? `${e.message ?? 'No recovery email is on file.'} Set one in Settings → Winlink Account.`
            : 'Could not send the recovery email. Check your connection and try again.';
        setRecoveryMsg({ ok: false, text });
      })
      .finally(() => setRecovering(false));
  }

  function handleFullRowClick(full: FullIdentityDto) {
    if (isActiveFull(full.callsign)) {
      closeDropdown();
      return;
    }
    revealUnlock({ kind: 'full', callsign: full.callsign });
  }

  function handleTacticalRowClick(t: TacticalIdentityDto) {
    if (isActiveTactical(t)) {
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
  function tacticalIsUnlocking(parent: string, label: string): boolean {
    return (
      unlockTarget?.kind === 'tactical' &&
      unlockTarget.label === label &&
      unlockTarget.parent === parent
    );
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
          {recoveryAvailable && (
            <button
              type="button"
              className="identity-forgot"
              data-testid="identity-forgot"
              onClick={sendRecovery}
              disabled={recovering}
            >
              {recovering ? 'Sending…' : 'Forgot password?'}
            </button>
          )}
        </div>
        {error && (
          <div className="identity-unlock-error" data-testid="identity-unlock-error" role="alert">
            {error}
          </div>
        )}
        {recoveryMsg && (
          <div
            className={recoveryMsg.ok ? 'identity-recovery-ok' : 'identity-unlock-error'}
            data-testid="identity-recovery-msg"
            role="status"
          >
            {recoveryMsg.text}
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
        className={`identity-switcher-trigger${locked ? ' identity-switcher-trigger--locked' : ''}`}
        data-testid="identity-switcher-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={
          locked
            ? `${primaryLabel} — locked: not authenticated for transmit. Open to authenticate.`
            : undefined
        }
        title={locked ? 'Not authenticated for transmit — open to authenticate your identity' : undefined}
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
        {locked && (
          <span className="identity-chip-lock" data-testid="identity-chip-lock" aria-hidden="true">
            🔒
          </span>
        )}
      </button>

      {open && coords && createPortal(
        <div
          ref={listRef}
          className="identity-switcher-list"
          data-testid="identity-switcher-list"
          role="listbox"
          tabIndex={-1}
          style={{ top: coords.top, left: coords.left, minWidth: coords.width }}
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
          ) : list.full.length === 0 ? (
            <div className="identity-list-empty" data-testid="identity-switcher-empty">
              No identities configured. Add one in Settings → Identities.
            </div>
          ) : (
            list.full.map((full) => {
              const tacticals = list.tactical.filter((t) => t.parent === full.callsign);
              const fullCurrent = list.last_selected != null && eqCI(list.last_selected, full.callsign);
              const fullLocked = full.needs_auth;
              return (
                <div key={`full-${full.callsign}`} className="identity-group">
                  <button
                    type="button"
                    role="option"
                    aria-selected={isActiveFull(full.callsign)}
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
                    const tCurrent = list.last_selected != null && eqCI(list.last_selected, t.label);
                    return (
                      <div key={`tactical-${t.parent}-${t.label}`} className="identity-group">
                        <button
                          type="button"
                          role="option"
                          aria-selected={isActiveTactical(t)}
                          aria-current={tCurrent ? 'true' : undefined}
                          aria-label={t.label}
                          className="identity-row identity-row--tactical"
                          data-testid={`identity-row-tactical-${t.label}`}
                          onClick={() => handleTacticalRowClick(t)}
                        >
                          <span className="identity-row-label">{t.label}</span>
                          {tacticalCmsBadge(t.cms_badge)}
                        </button>
                        {tacticalIsUnlocking(t.parent, t.label) && renderUnlockForm()}
                      </div>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>,
        document.body,
      )}
    </div>
  );
}
