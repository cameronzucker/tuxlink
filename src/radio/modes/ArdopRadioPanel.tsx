// src/radio/modes/ArdopRadioPanel.tsx
//
// Spec §5.3 — ARDOP Winlink panel. Replaces the legacy ArdopDock +
// ArdopHfStub pair (P4.6 deletes both). Composes RadioPanel chrome
// + Connect form + Live + Signal + Session log + Actions.
//
// Live data: useModemStatus subscribes to the backend's 4 Hz
// `modem:status` event stream. The S/N + throughput sparklines pull
// from rolling 60-sample buffers (`useSampleHistory`) that tick once
// per second off the latest status snapshot. Quality + recent-frame
// state come directly from ModemStatus (PINGACK-derived; tuxlink-1637,
// P4.3) and from a derived state-driven frame history.
//
// RADIO-1 SAFETY: every connect path passes through the consent
// modal + backend-minted token + atomic consume_consent_token gate.
// The frontend never generates tokens. After a connect (success or
// failure) the local token copy is cleared so the next Connect click
// re-opens the modal — preserved from ArdopDock.
//
// Open WebGUI: ardopcf's built-in WebGUI listens on `cmd_port - 1`
// per its USAGE doc. We read the live cmd_port from config rather
// than hardcoding 8514 so the link tracks operator overrides. Guard
// mirrors the backend's build_ardop_extra_args check: cmd_port < 2
// yields an unbindable webgui_port, so we surface an error rather
// than open a dead URL.

import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel, type RadioPanelState } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { SignalSection } from '../sections/SignalSection';
import { Sparkline } from '../charts/Sparkline';
import { useSampleHistory } from '../useSampleHistory';
import { useModemStatus } from '../../modem/useModemStatus';
import { useConsent } from '../../modem/useConsent';
import { ConsentModal } from '../../modem/ConsentModal';
import type { ModemState, ModemStatus } from '../../modem/types';
import type { ArdopFrameType } from '../charts/FrameRibbon';
import './ArdopRadioPanel.css';

export interface ArdopRadioPanelProps {
  onClose: () => void;
}

// ARQ state cells — same set the legacy ArdopDock surfaced; kept here
// because the new panel still shows the same 9-cell state strip.
const ARQ_CELLS = ['DISC', 'CON', 'IDLE', 'ISS', 'IRS', 'BUSY', 'RX', 'TX', 'DREQ'] as const;
type ArqCell = (typeof ARQ_CELLS)[number];

function isArqCellOn(cell: ArqCell, s: ModemStatus): boolean {
  switch (cell) {
    case 'DISC':
      return s.state === 'stopped' || s.state === 'idle' || s.state === 'disconnecting';
    case 'CON':
      return s.state === 'connected-irs' || s.state === 'connected-iss';
    case 'IDLE':
      return s.state === 'idle';
    case 'ISS':
      return s.state === 'connected-iss';
    case 'IRS':
      return s.state === 'connected-irs';
    case 'BUSY':
      return s.arqFlags.busy;
    case 'RX':
      return s.arqFlags.rx;
    case 'TX':
      return s.arqFlags.tx;
    case 'DREQ':
      return s.state === 'connecting';
  }
}

/**
 * Map the modem state machine into the RadioPanel chrome's `state` prop.
 * The chrome's state set is a coarser palette (connecting / connected /
 * disconnecting / error / disconnected) than the modem's 9-state machine.
 */
function mapModemStateToPanelState(modemState: ModemState): RadioPanelState {
  switch (modemState) {
    case 'stopped':
    case 'idle':
      return 'disconnected';
    case 'spawning':
    case 'initializing':
    case 'connecting':
      return 'connecting';
    case 'connected-irs':
    case 'connected-iss':
      return 'connected';
    case 'disconnecting':
      return 'disconnecting';
    case 'error':
      return 'error';
  }
}

/**
 * Derive a coarse ArdopFrameType from a ModemStatus snapshot. ardopcf does
 * not directly emit per-frame subprotocol-type events on the cmd socket
 * today; we approximate from the state machine + ARQ flags so the ribbon
 * still gives an at-a-glance read on recent on-air activity. A real
 * per-frame event stream is a future enhancement (see spec §5.3 follow-up).
 */
function frameTypeFromStatus(s: ModemStatus): ArdopFrameType {
  if (s.state === 'connecting') return 'CON';
  if (s.arqFlags.tx || s.arqFlags.rx) return 'DATA';
  if (s.state === 'connected-irs' || s.state === 'connected-iss') return 'ACK';
  if (s.state === 'error') return 'REJ';
  return 'IDLE';
}

/**
 * Rolling buffer of derived frame types. Captures one frame-type sample
 * per `intervalMs` tick (default 1000 ms) so the ribbon corresponds to
 * "last N seconds of activity," matching the S/N sparkline's cadence.
 *
 * Hold the latest status in a ref so the interval reads the freshest
 * snapshot without restarting the timer on every render.
 */
function useFrameHistory(
  status: ModemStatus,
  length: number,
  intervalMs: number = 1000,
): ArdopFrameType[] {
  const [frames, setFrames] = useState<ArdopFrameType[]>(() =>
    new Array(length).fill('IDLE' as ArdopFrameType),
  );
  const latest = useRef<ModemStatus>(status);
  latest.current = status;

  useEffect(() => {
    const id = setInterval(() => {
      setFrames((prev) => [...prev.slice(1), frameTypeFromStatus(latest.current)]);
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return frames;
}

function Meter({
  label,
  value,
  warn,
}: {
  label: string;
  value: string;
  warn?: boolean;
}) {
  return (
    <div className={`ardop-meter${warn ? ' warn' : ''}`} data-testid={`ardop-meter-${label.toLowerCase().replace(/[^a-z0-9]/g, '-')}`}>
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

// ARQ bandwidth options — wire shape expected by ardopcf's `ARQBW`. `null`
// renders as empty <option value=""> meaning "Auto (ardopcf default)".
// Mirrors SettingsPanel.tsx — restored to the Connect section per Codex P1
// 2026-05-31 so the operator doesn't have to leave the radio panel.
const ARQ_BANDWIDTH_OPTIONS: { value: number | null; label: string }[] = [
  { value: null, label: 'Auto (ardopcf default)' },
  { value: 200, label: '200 Hz (most robust)' },
  { value: 500, label: '500 Hz (marginal HF)' },
  { value: 1000, label: '1000 Hz' },
  { value: 2000, label: '2000 Hz (best throughput)' },
];

export function ArdopRadioPanel({ onClose }: ArdopRadioPanelProps) {
  const { status } = useModemStatus();
  const [target, setTarget] = useState('');
  // ARQ bandwidth (restored 2026-05-31 — Codex P1). Loaded from
  // config_get_ardop on mount; persisted via config_set_ardop on change.
  // null = "leave at ardopcf default."
  const [bandwidth, setBandwidth] = useState<number | null>(null);
  const consent = useConsent();
  const [showConsent, setShowConsent] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const [exchanging, setExchanging] = useState(false);

  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Rolling 60-sample buffers (1 Hz tick) for the S/N + throughput
  // sparklines. The hook reads the latest reading out of a ref every
  // tick, so the buffer always reflects the freshest modem snapshot.
  const snrHistory = useSampleHistory(status.snDb, 60);
  const throughputHistory = useSampleHistory(status.throughputBps, 60);
  const frameHistory = useFrameHistory(status, 60);

  // Load ARQ bandwidth from persisted ARDOP config on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<{ bandwidth_hz: number | null }>('config_get_ardop')
      .then((c) => {
        if (!cancelled && c && typeof c.bandwidth_hz !== 'undefined') {
          setBandwidth(c.bandwidth_hz);
        }
      })
      .catch(() => {
        /* pre-wizard / config absent — keep null default */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onBandwidthChange = (e: ChangeEvent<HTMLSelectElement>) => {
    // value="" represents the "Auto" option → null. Otherwise parse as int.
    const raw = e.target.value;
    const next = raw === '' ? null : parseInt(raw, 10);
    setBandwidth(next);
    // Persist via merge: read current ardop config, splice bandwidth, write
    // back. Mirrors SettingsPanel.tsx's pattern so two writers can't clobber
    // each other's fields.
    void (async () => {
      try {
        const current = await invoke<Record<string, unknown>>('config_get_ardop');
        await invoke('config_set_ardop', {
          value: { ...current, bandwidth_hz: next },
        });
      } catch {
        // Persist errors surface via the session log; UI keeps the new value.
      }
    })();
  };

  const isStopped = status.state === 'stopped';
  const isExchangeReady =
    status.state === 'connected-irs' || status.state === 'connected-iss';
  // Effective B2F target: ONLY the backend-reported peer authorizes a TX
  // target (preserved from ArdopDock; RADIO-1 hazard if we ever fall back
  // to the stopped-state `target` input).
  const effectiveTarget: string | null = status.peer?.trim() ?? null;

  const doConnect = async (tok: string) => {
    setConnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_connect', {
        target: target.trim(),
        consentToken: tok,
      });
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setConnecting(false);
      // RADIO-1 per-invocation consent: backend consumed the token in
      // consume_consent_token (atomic equality-check-and-clear). Clear
      // the local copy so the next Connect click re-opens the modal.
      // Preserved from ArdopDock.
      consent.clear();
    }
  };

  const onStartClick = () => {
    setConnectError(null);
    if (consent.token) {
      void doConnect(consent.token);
    } else {
      setShowConsent(true);
    }
  };

  const onSendReceiveClick = async () => {
    if (!isExchangeReady || effectiveTarget === null) return;
    setExchanging(true);
    setConnectError(null);
    try {
      // RADIO-1 SAFETY: token minted on BACKEND; frontend never generates.
      const tok = await invoke<string>('modem_mint_consent');
      await invoke('modem_ardop_b2f_exchange', {
        target: effectiveTarget,
        consentToken: tok,
      });
    } catch (e) {
      setConnectError(`Send/Receive failed: ${e}`);
    } finally {
      setExchanging(false);
    }
  };

  const onStopClick = async () => {
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

  const onOpenWebGuiClick = async () => {
    setConnectError(null);
    try {
      const ardop = await invoke<{ cmd_port: number }>('config_get_ardop');
      // Guard mirrors backend's build_ardop_extra_args: cmd_port < 2
      // yields an unbindable webgui_port. Surface that explicitly
      // rather than open a dead URL.
      if (ardop.cmd_port < 2) {
        setConnectError(
          `Cannot open WebGUI: ARDOP cmd_port=${ardop.cmd_port} is too low (need >= 2)`,
        );
        return;
      }
      const webguiPort = ardop.cmd_port - 1;
      window.open(`http://localhost:${webguiPort}/`, '_blank');
    } catch (e) {
      setConnectError(`Failed to open WebGUI: ${e}`);
    }
  };

  const onConsentConfirm = async () => {
    setShowConsent(false);
    try {
      // RADIO-1 SAFETY: token minted on backend; frontend never generates.
      const tok = await invoke<string>('modem_mint_consent');
      consent.grant(tok);
      void doConnect(tok);
    } catch (e) {
      setConnectError(`failed to mint consent token: ${e}`);
    }
  };

  const onTargetChange = (e: ChangeEvent<HTMLInputElement>) => {
    setTarget(e.target.value);
  };

  const headerSub = `${status.peer ?? '—'} · ${status.widthHz ? `${status.widthHz} Hz` : '—'}`;

  return (
    <RadioPanel
      mode={{ kind: 'ardop-hf', intent: 'cms' }}
      state={mapModemStateToPanelState(status.state)}
      sub={headerSub}
      onClose={onClose}
    >
      {isStopped && (
        <section className="radio-panel-sec">
          <h5>Connect</h5>
          <label className="radio-panel-input-row">
            <span>Target</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="ardop-target-input"
              value={target}
              onChange={onTargetChange}
              placeholder="W7RMS-10"
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Bandwidth</span>
            <select
              className="radio-panel-input"
              data-testid="ardop-bandwidth-select"
              value={bandwidth ?? ''}
              onChange={onBandwidthChange}
            >
              {ARQ_BANDWIDTH_OPTIONS.map((opt) => (
                <option key={String(opt.value)} value={opt.value ?? ''}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>
        </section>
      )}

      {!isStopped && (
        <section className="radio-panel-sec">
          <h5>ARQ state</h5>
          <div className="ardop-arq-grid">
            {ARQ_CELLS.map((cell) => (
              <div
                key={cell}
                className="ardop-arq-cell"
                data-testid={`arq-cell-${cell}`}
                data-on={isArqCellOn(cell, status)}
              >
                {cell}
              </div>
            ))}
          </div>
        </section>
      )}

      {!isStopped && (
        <section className="radio-panel-sec">
          <h5>Live</h5>
          {status.snDb !== null && (
            <Meter
              label="S/N"
              value={`${status.snDb > 0 ? '+' : ''}${status.snDb.toFixed(1)} dB`}
            />
          )}
          {status.vuDbfs !== null && (
            <Meter label="VU" value={`${status.vuDbfs.toFixed(0)} dBFS`} />
          )}
          {status.throughputBps !== null && (
            <Meter label="Throughput" value={`${status.throughputBps} bps`} warn />
          )}
          <Sparkline samples={throughputHistory} height={28} />
          <pre className="radio-panel-mono ardop-stats">
{`Peer   ${status.peer ?? '—'}
Mode   ${status.mode ?? '—'}
Width  ${status.widthHz !== null ? `${status.widthHz} Hz` : '—'}
PTT    ${status.pttBackend ?? '—'}
RX     ${status.bytesRx} B  ·  TX ${status.bytesTx} B
Up     ${fmtUptime(status.uptimeSec)}`}
          </pre>
        </section>
      )}

      <SignalSection
        quality={status.quality}
        snrSamples={snrHistory}
        recentFrames={frameHistory}
        snrCurrent={status.snDb}
      />

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      <section className="radio-panel-sec radio-panel-act">
        {isStopped && (
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-primary"
            data-testid="ardop-start-btn"
            disabled={target.trim() === '' || connecting}
            onClick={onStartClick}
          >
            {connecting ? 'Connecting…' : 'Start'}
          </button>
        )}
        {!isStopped && (
          <>
            <button
              type="button"
              className="radio-panel-btn radio-panel-btn-primary"
              data-testid="ardop-send-receive-btn"
              disabled={!isExchangeReady || exchanging || effectiveTarget === null}
              onClick={onSendReceiveClick}
            >
              {exchanging ? 'Exchanging…' : 'Send/Receive'}
            </button>
            <button
              type="button"
              className="radio-panel-btn radio-panel-btn-bad"
              data-testid="ardop-stop-btn"
              disabled={disconnecting}
              onClick={onStopClick}
            >
              {disconnecting ? 'Stopping…' : 'Stop'}
            </button>
          </>
        )}
        <button
          type="button"
          className="radio-panel-btn"
          data-testid="ardop-open-webgui-btn"
          onClick={onOpenWebGuiClick}
          title="Open ardopcf's built-in Spectrum/Waterfall view in browser"
        >
          Open WebGUI
        </button>
        {connectError !== null && (
          <p className="radio-panel-error" role="alert">{connectError}</p>
        )}
      </section>

      {showConsent && (
        <ConsentModal
          target={target.trim()}
          onCancel={() => setShowConsent(false)}
          onConfirm={onConsentConfirm}
        />
      )}
    </RadioPanel>
  );
}
