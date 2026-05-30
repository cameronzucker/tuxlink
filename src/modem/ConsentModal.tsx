import { useState } from 'react';

export interface ConsentModalProps {
  target: string;
  onCancel: () => void;
  onConfirm: () => void;
}

/**
 * RADIO-1 consent modal. The operator MUST tick the acknowledgement before
 * Connect becomes enabled. The token that authorizes the connect is minted
 * on the BACKEND (via `modem_mint_consent`) — this modal only collects the
 * operator's per-invocation consent; it does NOT generate tokens itself.
 * See `ArdopDock.onConsentConfirm` for the mint-then-store-then-connect wire.
 */
export function ConsentModal({ target, onCancel, onConfirm }: ConsentModalProps) {
  const [ack, setAck] = useState(false);
  return (
    <div className="ardop-consent-overlay" role="dialog" aria-modal="true">
      <div className="ardop-consent-modal">
        <h3>About to transmit on amateur radio</h3>
        <p>
          Target: <strong>{target}</strong>. Estimated airtime: ~2–8 minutes typical
          (depends on traffic). Frequency under operator control via your rig + ardopcf.
        </p>
        <label className="ardop-consent-ack">
          <input
            type="checkbox"
            checked={ack}
            onChange={(e) => setAck(e.target.checked)}
          />
          I confirm I am the licensee or authorized to operate under this callsign
          and authorize this transmission.
        </label>
        <div className="ardop-consent-actions">
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" disabled={!ack} onClick={onConfirm}>Connect</button>
        </div>
      </div>
    </div>
  );
}
