// src/radio/modes/VaraRadioPanel.tsx
//
// Phase 2 (bd-tuxlink-dfmf) — VARA HF / VARA FM panel. Conservative scope:
// open/close the TCP transport to the operator's VARA instance, surface the
// connect/error state, edit the persisted VaraUiConfig. No RF connect-to-peer
// yet — that path needs the session state machine + RADIO-1 consent flow,
// both Phase 3 deliverables.
//
// Mode awareness: the panel renders the same controls for `vara-hf` and
// `vara-fm` — the operator picks the variant via which VARA instance they
// point tuxlink at (different cmd_port). The mode prop drives only the
// panel header title.
//
// Pi-availability (tuxlink-xfo): on aarch64 hosts (Pi 5), Wine cannot run
// VARA — the panel reads `platform_info.vara_supported` and renders a
// disabled-with-banner state so the operator understands why the controls
// are unusable. The Start button is disabled regardless of the form state.

import { useEffect, useState } from 'react';
import type { ChangeEvent, KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel, type RadioPanelState } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { useVaraConfig } from '../useVaraConfig';
import type { VaraUiConfig } from '../useVaraConfig';
import type { RadioPanelMode } from '../types';

/** Mirror of Rust's `commands::VaraStatus`. camelCase per the Rust
 *  `#[serde(rename_all = "camelCase")]` on the struct. */
interface VaraStatusDto {
  state: 'closed' | 'connecting' | 'open' | 'error';
  lastError: string | null;
  boundHost: string | null;
  boundCmdPort: number | null;
}

/** Mirror of Rust's `commands::PlatformInfo`. */
interface PlatformInfoDto {
  arch: string;
  os: string;
  varaSupported: boolean;
}

export interface VaraRadioPanelProps {
  mode: RadioPanelMode;
  onClose: () => void;
}

/** Documented bandwidth presets. The selector lets the operator pick one of
 *  these and persists `bandwidth_hz`. Empty (string "") = "leave at VARA's
 *  default" — the start command skips the BW setter in that case. */
const BANDWIDTH_OPTIONS: { value: number | ''; label: string }[] = [
  { value: '', label: 'Auto (VARA default)' },
  { value: 500, label: '500 Hz (narrow HF)' },
  { value: 2300, label: '2300 Hz (HF Standard)' },
  { value: 2750, label: '2750 Hz (HF Tactical)' },
];

function mapVaraStateToPanelState(s: VaraStatusDto['state']): RadioPanelState {
  switch (s) {
    case 'closed':
      return 'disconnected';
    case 'connecting':
      return 'connecting';
    case 'open':
      return 'connected';
    case 'error':
      return 'error';
  }
}

export function VaraRadioPanel({ mode, onClose }: VaraRadioPanelProps) {
  const { config, setConfig, loading } = useVaraConfig();
  const [status, setStatus] = useState<VaraStatusDto>({
    state: 'closed',
    lastError: null,
    boundHost: null,
    boundCmdPort: null,
  });
  const [platform, setPlatform] = useState<PlatformInfoDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  // Local input mirrors so the operator can type freely; commit on blur.
  const [hostInput, setHostInput] = useState<string>('');
  const [cmdPortInput, setCmdPortInput] = useState<string>('');
  const [dataPortInput, setDataPortInput] = useState<string>('');

  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Hydrate inputs from the loaded config. Re-runs when config changes (e.g.
  // a peer hook persisted an update via the same-window CustomEvent).
  useEffect(() => {
    setHostInput(config.host);
    setCmdPortInput(String(config.cmd_port));
    setDataPortInput(String(config.data_port));
  }, [config.host, config.cmd_port, config.data_port]);

  // Load platform info once on mount for the Pi-availability gating.
  useEffect(() => {
    let cancelled = false;
    invoke<PlatformInfoDto>('platform_info')
      .then((p) => {
        if (!cancelled) setPlatform(p);
      })
      .catch(() => {
        // platform_info has no failure path in practice (cfg!-based, no
        // I/O). If it's missing for some reason (older backend in dev),
        // err on the side of permissive — leave platform=null, which
        // does NOT disable the controls.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Load the initial status on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<VaraStatusDto>('vara_status')
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {
        /* No-op — status defaults to closed. */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const platformBlocked = platform !== null && !platform.varaSupported;

  const commitHost = () => {
    const trimmed = hostInput.trim();
    if (!trimmed) {
      setHostInput(config.host); // revert
      setActionError('Host cannot be empty — reverted.');
      return;
    }
    if (trimmed === config.host) return;
    setConfig({ ...config, host: trimmed });
  };

  const commitPort = (
    raw: string,
    field: 'cmd_port' | 'data_port',
    setInput: (s: string) => void,
  ) => {
    const trimmed = raw.trim();
    const n = Number(trimmed);
    if (!Number.isInteger(n) || n < 1 || n > 65535) {
      setInput(String(config[field]));
      setActionError(`Invalid ${field.replace('_', ' ')} "${trimmed}" — must be 1..65535. Reverted.`);
      return;
    }
    if (n === config[field]) return;
    setConfig({ ...config, [field]: n });
  };

  const onBandwidthChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const raw = e.target.value;
    const next: VaraUiConfig = {
      ...config,
      bandwidth_hz: raw === '' ? null : parseInt(raw, 10),
    };
    setConfig(next);
  };

  const onPortKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const onStartClick = async () => {
    if (busy || platformBlocked) return;
    setBusy(true);
    setActionError(null);
    try {
      const next = await invoke<VaraStatusDto>('vara_start_session');
      setStatus(next);
    } catch (e) {
      setActionError(`Start failed: ${String(e)}`);
      // Refresh status so a backend-side Error state surfaces.
      try {
        const s = await invoke<VaraStatusDto>('vara_status');
        setStatus(s);
      } catch {
        /* keep prior status */
      }
    } finally {
      setBusy(false);
    }
  };

  const onStopClick = async () => {
    if (busy) return;
    setBusy(true);
    setActionError(null);
    try {
      const next = await invoke<VaraStatusDto>('vara_stop_session');
      setStatus(next);
    } catch (e) {
      setActionError(`Stop failed: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const headerSub = status.boundHost
    ? `${status.boundHost}:${status.boundCmdPort ?? '?'}`
    : `${hostInput || config.host}:${cmdPortInput || config.cmd_port}`;

  const isOpen = status.state === 'open' || status.state === 'connecting';

  return (
    <RadioPanel
      mode={mode}
      state={mapVaraStateToPanelState(status.state)}
      sub={headerSub}
      onClose={onClose}
    >
      {platformBlocked && (
        <section className="radio-panel-sec" data-testid="vara-platform-banner">
          <p className="radio-panel-error" role="alert">
            VARA requires an x86 / x86_64 host. This machine is{' '}
            <code>{platform?.arch}</code> ({platform?.os}); Wine cannot run VARA on this
            architecture (Pi 5 16K-page kernel blocks Wine entirely). Controls below
            are disabled. Run tuxlink on an x86 / x86_64 machine to use VARA, or use
            ARDOP HF / Packet on this host.
          </p>
        </section>
      )}

      <section className="radio-panel-sec">
        <h5>VARA host</h5>
        <label className="radio-panel-input-row">
          <span>Host</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="vara-host-input"
            value={hostInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="127.0.0.1"
            disabled={loading || isOpen || platformBlocked}
            onChange={(e) => setHostInput(e.target.value)}
            onBlur={commitHost}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Cmd port</span>
          <input
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="vara-cmd-port-input"
            value={cmdPortInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="8300"
            disabled={loading || isOpen || platformBlocked}
            onChange={(e) => setCmdPortInput(e.target.value)}
            onBlur={() => commitPort(cmdPortInput, 'cmd_port', setCmdPortInput)}
            onKeyDown={onPortKey}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Data port</span>
          <input
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="vara-data-port-input"
            value={dataPortInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="8301"
            disabled={loading || isOpen || platformBlocked}
            onChange={(e) => setDataPortInput(e.target.value)}
            onBlur={() => commitPort(dataPortInput, 'data_port', setDataPortInput)}
            onKeyDown={onPortKey}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Bandwidth</span>
          <select
            className="radio-panel-input"
            data-testid="vara-bandwidth-select"
            value={config.bandwidth_hz ?? ''}
            disabled={loading || isOpen || platformBlocked}
            onChange={onBandwidthChange}
          >
            {BANDWIDTH_OPTIONS.map((opt) => (
              <option key={String(opt.value)} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </label>
      </section>

      <section className="radio-panel-sec" data-testid="vara-status-section">
        <h5>Transport</h5>
        <p className="radio-panel-mono" data-testid="vara-state-display">
          {`State: ${status.state}`}
        </p>
        {status.lastError && (
          <p className="radio-panel-error" data-testid="vara-last-error">
            {status.lastError}
          </p>
        )}
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          data-testid="vara-start-btn"
          disabled={busy || loading || isOpen || platformBlocked}
          onClick={onStartClick}
          title={
            platformBlocked
              ? 'VARA cannot run on this architecture (see banner above)'
              : isOpen
                ? 'Already open — Stop first to reconnect'
                : 'Open TCP transport to VARA (does not transmit)'
          }
        >
          {busy && !isOpen ? 'Starting…' : 'Start'}
        </button>
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-bad"
          data-testid="vara-stop-btn"
          disabled={busy || !isOpen}
          onClick={onStopClick}
        >
          {busy && isOpen ? 'Stopping…' : 'Stop'}
        </button>
        {actionError && (
          <p className="radio-panel-error" role="alert" data-testid="vara-action-error">
            {actionError}
          </p>
        )}
      </section>
    </RadioPanel>
  );
}
