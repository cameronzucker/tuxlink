// IdentitiesSettings — the Settings-chrome section for managing FULL (licensed)
// + tactical identities (the multi/tactical-callsign feature's management GUI,
// bd-tuxlink-z6yi). The backend identity commands shipped in Phase 2; this is
// the missing operator surface that wires them.
//
// Lists each FULL identity with its tacticals nested beneath, and offers two
// add forms (FULL, tactical) plus per-row removal with inline confirmation.
// Adding a CMS FULL writes BOTH the CMS keyring password AND the store record +
// activation secret (design-review F2 — handled inside useAddFullIdentity).
//
// Inline, in-chrome (operator pet-peeve: no popup windows; no stretched
// full-width inputs — widths are constrained in the CSS).

import { useMemo, useState } from 'react';
import {
  useIdentityList,
  useAddFullIdentity,
  useAddTactical,
  useRemoveIdentity,
} from './useIdentities';
import { parseIdentityError, type CmsBadge } from './identityTypes';
import { cmsPasswordTruncationNotice } from '../wizard/validators';
import './IdentitiesSettings.css';

const CMS_BADGE_TEXT: Record<CmsBadge, string> = {
  registered: 'CMS registered',
  not_registered: 'no CMS',
  unknown: 'CMS unknown',
};

export function IdentitiesSettings() {
  const list = useIdentityList();
  const addFull = useAddFullIdentity();
  const addTactical = useAddTactical();
  const removeIdentity = useRemoveIdentity();

  const [error, setError] = useState<string | null>(null);

  // Add-FULL form state.
  const [callsign, setCallsign] = useState('');
  const [password, setPassword] = useState('');
  const [label, setLabel] = useState('');
  const [hasCmsAccount, setHasCmsAccount] = useState(true);

  // Add-tactical form state.
  const [tacticalLabel, setTacticalLabel] = useState('');
  const [tacticalParent, setTacticalParent] = useState('');

  // Which row is awaiting remove-confirmation (callsign or tactical label).
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);

  const fulls = list.data?.full ?? [];
  const tacticals = list.data?.tactical ?? [];
  const hasIdentities = fulls.length > 0 || tacticals.length > 0;

  // Default the tactical parent select to the first FULL once the list loads.
  const parentValue = tacticalParent || fulls[0]?.callsign || '';

  const tacticalsByParent = useMemo(() => {
    const m = new Map<string, typeof tacticals>();
    for (const t of tacticals) {
      const arr = m.get(t.parent) ?? [];
      arr.push(t);
      m.set(t.parent, arr);
    }
    return m;
  }, [tacticals]);

  async function handleAddFull(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const cs = callsign.trim();
    if (!cs) {
      setError('Enter a callsign.');
      return;
    }
    try {
      await addFull.mutateAsync({
        callsign: cs,
        label: label.trim() || null,
        hasCmsAccount,
        password,
      });
      setCallsign('');
      setPassword('');
      setLabel('');
      setHasCmsAccount(true);
    } catch (err) {
      setError(parseIdentityError(err));
    }
  }

  async function handleAddTactical(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const lbl = tacticalLabel.trim();
    if (!lbl) {
      setError('Enter a tactical label.');
      return;
    }
    if (!parentValue) {
      setError('Select a parent callsign.');
      return;
    }
    try {
      await addTactical.mutateAsync({ label: lbl, parent: parentValue });
      setTacticalLabel('');
    } catch (err) {
      setError(parseIdentityError(err));
    }
  }

  async function handleRemove(args:
    | { kind: 'full'; callsign: string }
    | { kind: 'tactical'; label: string }) {
    setError(null);
    setConfirmingRemove(null);
    try {
      await removeIdentity.mutateAsync(args);
    } catch (err) {
      setError(parseIdentityError(err));
    }
  }

  return (
    <div className="tux-identities" data-testid="identities-settings">
      {error && (
        <p className="tux-identities__error" role="alert" data-testid="identities-error">
          {error}
        </p>
      )}

      {!hasIdentities ? (
        <p className="tux-identities__empty">
          No identities configured. Add one to send and receive.
        </p>
      ) : (
        <ul className="tux-identities__list">
          {fulls.map((f) => {
            const kids = tacticalsByParent.get(f.callsign) ?? [];
            return (
              <li
                key={f.callsign}
                className="tux-identities__full"
                data-testid={`identity-row-full-${f.callsign}`}
              >
                <div className="tux-identities__full-head">
                  <span className="tux-identities__callsign">{f.callsign}</span>
                  {f.label && <span className="tux-identities__label">{f.label}</span>}
                  <span className="tux-identities__badge">
                    {f.has_cms_account ? CMS_BADGE_TEXT[f.cms_registered ? 'registered' : 'unknown'] : 'no CMS'}
                  </span>
                  <RemoveControl
                    id={f.callsign}
                    confirming={confirmingRemove === f.callsign}
                    onAskConfirm={() => setConfirmingRemove(f.callsign)}
                    onCancel={() => setConfirmingRemove(null)}
                    onConfirm={() => handleRemove({ kind: 'full', callsign: f.callsign })}
                  />
                </div>
                {kids.length > 0 && (
                  <ul className="tux-identities__tacticals">
                    {kids.map((t) => (
                      <li
                        key={t.label}
                        className="tux-identities__tactical"
                        data-testid={`identity-row-tactical-${t.label}`}
                      >
                        <span className="tux-identities__tactical-label">{t.label}</span>
                        <span className="tux-identities__badge">{CMS_BADGE_TEXT[t.cms_badge]}</span>
                        <RemoveControl
                          id={t.label}
                          confirming={confirmingRemove === t.label}
                          onAskConfirm={() => setConfirmingRemove(t.label)}
                          onCancel={() => setConfirmingRemove(null)}
                          onConfirm={() => handleRemove({ kind: 'tactical', label: t.label })}
                        />
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <form
        className="tux-identities__form"
        data-testid="identity-add-full-form"
        onSubmit={handleAddFull}
      >
        <h4 className="tux-identities__form-title">Add identity</h4>
        <label className="tux-identities__field">
          <span>Callsign</span>
          <input
            type="text"
            className="tux-identities__input"
            data-testid="identity-add-full-callsign"
            value={callsign}
            onChange={(e) => setCallsign(e.target.value)}
            autoComplete="off"
          />
        </label>
        <label className="tux-identities__field">
          <span>Label (optional)</span>
          <input
            type="text"
            className="tux-identities__input"
            data-testid="identity-add-full-label"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            autoComplete="off"
          />
        </label>
        <label className="tux-identities__field">
          <span>Password</span>
          <input
            type="password"
            className="tux-identities__input"
            data-testid="identity-add-full-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="off"
          />
          {hasCmsAccount && cmsPasswordTruncationNotice(password) && (
            <span
              className="tux-identities__notice"
              data-testid="identity-add-full-truncation-notice"
            >
              {cmsPasswordTruncationNotice(password)}
            </span>
          )}
        </label>
        <label className="tux-identities__check">
          <input
            type="checkbox"
            checked={hasCmsAccount}
            onChange={(e) => setHasCmsAccount(e.target.checked)}
          />
          <span>Has CMS account</span>
        </label>
        <button
          type="submit"
          className="tux-identities__btn"
          data-testid="identity-add-full-submit"
        >
          Add identity
        </button>
      </form>

      {fulls.length > 0 && (
        <form
          className="tux-identities__form"
          data-testid="identity-add-tactical-form"
          onSubmit={handleAddTactical}
        >
          <h4 className="tux-identities__form-title">Add tactical</h4>
          <label className="tux-identities__field">
            <span>Tactical label</span>
            <input
              type="text"
              className="tux-identities__input"
              data-testid="identity-add-tactical-label"
              value={tacticalLabel}
              onChange={(e) => setTacticalLabel(e.target.value)}
              autoComplete="off"
            />
          </label>
          <label className="tux-identities__field">
            <span>Parent callsign</span>
            <select
              className="tux-identities__select"
              data-testid="identity-add-tactical-parent"
              value={parentValue}
              onChange={(e) => setTacticalParent(e.target.value)}
            >
              {fulls.map((f) => (
                <option key={f.callsign} value={f.callsign}>
                  {f.callsign}
                </option>
              ))}
            </select>
          </label>
          <button
            type="submit"
            className="tux-identities__btn"
            data-testid="identity-add-tactical-submit"
          >
            Add tactical
          </button>
        </form>
      )}
    </div>
  );
}

interface RemoveControlProps {
  id: string;
  confirming: boolean;
  onAskConfirm: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}

function RemoveControl({ id, confirming, onAskConfirm, onCancel, onConfirm }: RemoveControlProps) {
  if (confirming) {
    return (
      <span className="tux-identities__confirm">
        <span className="tux-identities__confirm-text">Remove?</span>
        <button
          type="button"
          className="tux-identities__btn tux-identities__btn--danger"
          data-testid={`identity-remove-${id}-confirm`}
          onClick={onConfirm}
        >
          Confirm
        </button>
        <button
          type="button"
          className="tux-identities__btn tux-identities__btn--subtle"
          data-testid={`identity-remove-${id}-cancel`}
          onClick={onCancel}
        >
          Cancel
        </button>
      </span>
    );
  }
  return (
    <button
      type="button"
      className="tux-identities__btn tux-identities__btn--subtle"
      data-testid={`identity-remove-${id}`}
      onClick={onAskConfirm}
    >
      Remove
    </button>
  );
}
