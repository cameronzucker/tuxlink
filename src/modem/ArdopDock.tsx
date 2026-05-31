import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useModemStatus } from './useModemStatus';
import { useConsent } from './useConsent';
import { ConsentModal } from './ConsentModal';
import type { ModemStatus } from './types';
import './ArdopDock.css';

const ARQ_CELLS = ['DISC', 'CON', 'IDLE', 'ISS', 'IRS', 'BUSY', 'RX', 'TX', 'DREQ'] as const;
type ArqCell = (typeof ARQ_CELLS)[number];

function isCellOn(cell: ArqCell, s: ModemStatus): boolean {
  switch (cell) {
    case 'DISC':  return s.state === 'stopped' || s.state === 'idle' || s.state === 'disconnecting';
    case 'CON':   return s.state === 'connected-irs' || s.state === 'connected-iss';
    case 'IDLE':  return s.state === 'idle';
    case 'ISS':   return s.state === 'connected-iss';
    case 'IRS':   return s.state === 'connected-irs';
    case 'BUSY':  return s.arqFlags.busy;
    case 'RX':    return s.arqFlags.rx;
    case 'TX':    return s.arqFlags.tx;
    case 'DREQ':  return s.state === 'connecting';
  }
}

function Meter({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div className={`ardop-meter${warn ? ' warn' : ''}`}>
      <span className="ardop-meter-k">{label}</span>
      <span className="ardop-meter-v">{value}</span>
    </div>
  );
}

function fmtUptime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return m === 0 ? `${s}s` : `${m}m ${s}s`;
}

export function ArdopDock() {
  const { status } = useModemStatus();
  const [target, setTarget] = useState('');
  const consent = useConsent();
  const [showConsent, setShowConsent] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const [exchanging, setExchanging] = useState(false);

  const doConnect = async (tok: string) => {
    setConnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_connect', { target: target.trim(), consentToken: tok });
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setConnecting(false);
      // RADIO-1 per-invocation consent: the backend consumed the token in
      // `consume_consent_token` (atomic equality-check-and-clear). Clear the
      // local copy so the next Connect click re-opens the consent modal
      // regardless of whether this attempt succeeded or failed. Without
      // this, the `onConnectClick` shortcut at `if (consent.token)` would
      // re-submit a now-invalid token (backend would Err) AND skip the
      // modal — confusing UX and a stale-acknowledgement risk if the
      // backend ever softened the gate. Closes the 2026-05-30 Codex P1
      // finding on `ArdopDock.tsx:61-64`.
      consent.clear();
    }
  };

  const onConnectClick = () => {
    // Clear any prior error so a fresh attempt presents a clean dock — both
    // when we go straight to doConnect (consent token still cached) and when
    // we open the modal. Without this, a previous failed-connect's red error
    // banner stayed visible behind the modal (tuxlink-qvl §B).
    setConnectError(null);
    if (consent.token) {
      void doConnect(consent.token);
    } else {
      setShowConsent(true);
    }
  };

  // RADIO-1 SAFETY: this handler is the on-air-transmitting trigger for the
  // B2F mail exchange. The consent token MUST be backend-minted via
  // `modem_mint_consent` (mirrors the Connect path); the backend's
  // `consume_consent_token` atomic equality-check-and-clear is the actual
  // gate, but we also never want even the *appearance* of a client-side
  // mint (e.g. Math.random / crypto.randomUUID). The grep guard in the
  // commit body documents this property.
  const isExchangeReady =
    status.state === 'connected-irs' || status.state === 'connected-iss';
  // Effective B2F target: in the running view there is no input field
  // (the Connect form is unmounted), so we read the canonical peer the
  // backend reports. Falls back to the operator's typed `target` only in
  // the unlikely case `status.peer` is null while ConnectedIrs/Iss — the
  // typed value persists across the form unmount via the same `useState`.
  const effectiveTarget = (status.peer ?? target).trim();

  const onSendReceiveClick = async () => {
    if (!isExchangeReady || effectiveTarget === '') return;
    setExchanging(true);
    setConnectError(null);
    try {
      const tok = await invoke<string>('modem_mint_consent');
      await invoke('modem_ardop_b2f_exchange', {
        target: effectiveTarget,
        consentToken: tok,
      });
      // Success — backend has already cleaned up (disconnect + reset_to_stopped).
      // The next modem:status event will reflect Stopped state, the dock will
      // return to the Connect form. No further frontend mailbox-refresh
      // coordination here; the mailbox view picks up new messages on next
      // load.
    } catch (e) {
      setConnectError(`Send/Receive failed: ${e}`);
    } finally {
      setExchanging(false);
    }
  };

  const onDisconnectClick = async () => {
    setDisconnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_disconnect');
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setDisconnecting(false);
    }
  };

  const onConsentConfirm = async () => {
    setShowConsent(false);
    try {
      // RADIO-1 SAFETY: token minted on backend; frontend never generates it.
      // A frontend-generated token would let a compromised renderer self-mint
      // and bypass the consent gate (the backend rejects unknown tokens, but
      // we never want even the *appearance* of a client-side mint path).
      const tok = await invoke<string>('modem_mint_consent');
      consent.grant(tok);
      void doConnect(tok);
    } catch (e) {
      setConnectError(`failed to mint consent token: ${e}`);
    }
  };

  return (
    <aside className="ardop-dock" data-testid="ardop-dock-root">
      <header className="ardop-dock-h">
        <span className="ardop-dock-state-dot" data-state={status.state} />
        <span className="ardop-dock-name">MODEM · ARDOP HF</span>
        <span className="ardop-dock-sub">ardopcf · :8515</span>
      </header>

      {status.state === 'stopped' && (
        <section className="ardop-dock-section">
          <div className="ardop-dock-section-h">Target station</div>
          <label className="ardop-dock-field">
            Target callsign
            <input
              className="ardop-dock-input"
              data-testid="ardop-target"
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="W7RMS-10"
            />
          </label>
          <button
            type="button"
            className="ardop-dock-btn ardop-dock-btn-primary"
            disabled={target.trim() === '' || connecting}
            onClick={onConnectClick}
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </button>
          {connectError !== null && (
            <p className="ardop-dock-error" role="alert">{connectError}</p>
          )}
        </section>
      )}

      {status.state !== 'stopped' && (
        <>
          <section className="ardop-dock-section">
            <div className="ardop-dock-section-h">ARQ state</div>
            <div className="ardop-arq-grid">
              {ARQ_CELLS.map((cell) => (
                <div
                  key={cell}
                  className="ardop-arq-cell"
                  data-testid={`arq-cell-${cell}`}
                  data-on={isCellOn(cell, status)}
                >
                  {cell}
                </div>
              ))}
            </div>
          </section>

          <section className="ardop-dock-section">
            <div className="ardop-dock-section-h">Live</div>
            {status.snDb !== null && (
              <Meter label="S/N" value={`${status.snDb > 0 ? '+' : ''}${status.snDb.toFixed(1)} dB`} />
            )}
            {status.vuDbfs !== null && (
              <Meter label="VU input" value={`${status.vuDbfs.toFixed(0)} dBFS`} />
            )}
            {status.throughputBps !== null && (
              <Meter label="Throughput" value={`${status.throughputBps} bps`} warn />
            )}
          </section>

          <section className="ardop-dock-section">
            <pre className="ardop-mono-stat">
{`Peer   ${status.peer ?? '—'}
Mode   ${status.mode ?? '—'}
Width  ${status.widthHz !== null ? `${status.widthHz} Hz` : '—'}
PTT    ${status.pttBackend ?? '—'}
RX     ${status.bytesRx} B  ·  TX ${status.bytesTx} B
Up     ${fmtUptime(status.uptimeSec)}`}
            </pre>
          </section>

          <section className="ardop-dock-section">
            <button
              type="button"
              className="ardop-dock-btn ardop-dock-btn-primary"
              disabled={!isExchangeReady || exchanging || effectiveTarget === ''}
              onClick={onSendReceiveClick}
            >
              {exchanging ? 'Exchanging…' : 'Send/Receive'}
            </button>
            <button
              type="button"
              className="ardop-dock-btn"
              disabled={disconnecting}
              onClick={onDisconnectClick}
            >
              {disconnecting ? 'Disconnecting…' : 'Disconnect'}
            </button>
            {connectError !== null && (
              <p className="ardop-dock-error" role="alert">{connectError}</p>
            )}
          </section>
        </>
      )}

      {showConsent && (
        <ConsentModal
          target={target.trim()}
          onCancel={() => setShowConsent(false)}
          onConfirm={onConsentConfirm}
        />
      )}
    </aside>
  );
}
