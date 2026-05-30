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

  const doConnect = async (tok: string) => {
    setConnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_connect', { target: target.trim(), consentToken: tok });
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setConnecting(false);
    }
  };

  const onConnectClick = () => {
    if (consent.token) {
      void doConnect(consent.token);
    } else {
      setShowConsent(true);
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
