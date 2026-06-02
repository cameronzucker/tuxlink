// src/radio/modes/PacketRadioPanel.tsx
//
// Packet (AX.25) right-panel implementation per spec §5.2. Replaces the
// reading-pane PacketConnectionPanel — same affordances (modem link
// editor, station identity, optional Listen, Connect with digipeater
// path) trimmed for the 360 px right-panel column.
//
// Composition:
//   - ModemLinkSection (shared section, P3 task 3.1) for TCP/USB/BT
//     transport.
//   - Station sub-section: base callsign (read-only) + SSID picker;
//     SSID changes persist via packet_config_set.
//   - Listen sub-section (intent='p2p' only): arms packet_listen.
//   - Connect sub-section: target callsign + 0-2 relay chips; fires
//     packet_connect on click.
//   - SessionLogSection at the bottom of the body.
//   - Per-mode actions: Connect (primary) lives inside the Connect sub-
//     section; the bottom action row stays minimal so the body's section
//     ordering matches the Telnet panel.
//
// Persistence shape: every editable field calls packet_config_set with
// the full DTO. The component holds the loaded config in state; if the
// initial packet_config_get rejects (pre-wizard / no config yet), the
// panel renders with fallback defaults and persistence is a no-op until
// a config exists.

import { useEffect, useState } from 'react';
import type { ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { ModemLinkSection, type ModemLinkFields } from '../sections/ModemLinkSection';
import { effectiveCall, pathPreview, ssidOptions } from '../../packet/packetConfig';
import type { PacketConfigDto } from '../../packet/packetTypes';

export interface PacketRadioPanelProps {
  /** Session intent — governs whether Listen is shown.
   *   - 'cms': CMS-gateway outbound; Listen hidden (connect-only).
   *   - 'p2p': peer-to-peer; Listen shown.
   */
  intent: 'cms' | 'p2p';
  /** Operator base callsign from identity; '' until loaded. */
  baseCall: string;
  /** Called when the operator closes the panel. */
  onClose: () => void;
}

export function PacketRadioPanel({ intent, baseCall, onClose }: PacketRadioPanelProps) {
  const [config, setConfig] = useState<PacketConfigDto | null>(null);
  const [target, setTarget] = useState('');
  const [relays, setRelays] = useState<string[]>([]);
  const [armed, setArmed] = useState(false);
  // listenDefault PREFERENCE (auto-arm on startup) — distinct from the
  // live `armed` state above. Synced from config on load + persisted via
  // packet_set_listen. Restored 2026-05-31 from legacy PacketConnectionPanel.
  const [listenDefault, setListenDefault] = useState<boolean>(true);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Load packet config on mount. If packet_config_get rejects (pre-wizard)
  // we leave config=null and the panel renders with fallback defaults; any
  // persist attempts gate on `config !== null`.
  useEffect(() => {
    let cancelled = false;
    invoke<PacketConfigDto>('packet_config_get')
      .then((c) => {
        if (cancelled) return;
        setConfig(c);
        // Sync the listenDefault preference from config — keep panel
        // state in sync with persisted preference on first load.
        if (typeof c.listenDefault === 'boolean') {
          setListenDefault(c.listenDefault);
        }
      })
      .catch(() => {
        // Pre-wizard / no config — fallback defaults via getters below.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onToggleListenDefault = () => {
    const next = !listenDefault;
    setListenDefault(next);
    void invoke('packet_set_listen', { enabled: next }).catch(() => {
      // Rollback on failure so the checkbox doesn't lie about persisted state.
      setListenDefault((v) => !v);
    });
  };

  // Persist helper — merges new fields into the current DTO and writes
  // to backend. No-op when config is unloaded (we can't write a partial
  // DTO; the backend's #[serde(deny_unknown_fields)] would reject).
  // Also broadcasts via a same-window CustomEvent so the DashboardRibbon's
  // shared usePacketConfig hook sees SSID changes immediately (operator
  // smoke 2026-05-31 — the ribbon callsign was stuck at `<base>-0`).
  const persistDto = (next: PacketConfigDto) => {
    setConfig(next);
    if (typeof window !== 'undefined') {
      window.dispatchEvent(
        new CustomEvent('tuxlink:packet-config:change', { detail: next }),
      );
    }
    void invoke('packet_config_set', { dto: next }).catch(() => {
      // Persist errors surface in the session log via the backend.
    });
  };

  const ssid = config?.ssid ?? 0;

  const onSsidChange = (e: ChangeEvent<HTMLSelectElement>) => {
    if (!config) return;
    const n = Number(e.target.value);
    persistDto({ ...config, ssid: n });
  };

  const onLinkChange = (fields: ModemLinkFields) => {
    if (!config) return;
    persistDto({ ...config, ...fields });
  };

  const onAddRelay = () => setRelays((r) => (r.length < 2 ? [...r, ''] : r));
  const onRelayChange = (i: number, v: string) =>
    setRelays((r) => r.map((x, idx) => (idx === i ? v : x)));
  const onRemoveRelay = (i: number) => setRelays((r) => r.filter((_, idx) => idx !== i));

  const onListen = () => {
    if (armed) {
      // Stop: shut the link so a blocked answer() unwinds (Cancelled).
      void invoke('cms_abort').catch(() => {});
      setArmed(false);
      return;
    }
    setArmed(true);
    void invoke('packet_listen')
      .catch(() => {})
      .finally(() => {
        // Whether the call was answered, the exchange completed, or it
        // was stopped, we're no longer waiting.
        setArmed(false);
      });
  };

  const onConnect = () => {
    const call = target.trim();
    if (!call) return;
    const path = relays.map((r) => r.trim()).filter(Boolean);
    void invoke('packet_connect', { call, path }).catch(() => {});
  };

  const headerSub = config
    ? config.linkKind === 'Tcp'
      ? `${config.tcpHost ?? '127.0.0.1'}:${config.tcpPort ?? 8001}`
      : config.linkKind === 'Bluetooth'
      ? `BT ${config.btMac ?? '(no device)'}`
      : `${config.serialDevice ?? '(no device)'}`
    : undefined;

  return (
    <RadioPanel
      mode={{ kind: 'packet', intent }}
      state="disconnected"
      sub={headerSub}
      onClose={onClose}
    >
      <ModemLinkSection
        kind={
          config?.linkKind === 'Bluetooth'
            ? 'Bluetooth'
            : config?.linkKind === 'Serial'
            ? 'Serial'
            : 'Tcp'
        }
        host={config?.tcpHost ?? undefined}
        port={config?.tcpPort ?? undefined}
        serialDevice={config?.serialDevice ?? undefined}
        serialBaud={config?.serialBaud ?? undefined}
        btMac={config?.btMac ?? undefined}
        onChange={onLinkChange}
      />

      <section className="radio-panel-sec">
        <h5>My station</h5>
        <label className="radio-panel-input-row">
          <span>Call</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="packet-base-call"
            value={baseCall}
            readOnly
          />
        </label>
        <label className="radio-panel-input-row">
          <span>SSID</span>
          <select
            className="radio-panel-input"
            data-testid="packet-ssid-select"
            value={ssid}
            onChange={onSsidChange}
          >
            {ssidOptions().map((n) => (
              <option key={n} value={n}>{`-${n}`}</option>
            ))}
          </select>
        </label>
        <p
          className="radio-panel-mono"
          data-testid="packet-station-hint"
        >
          Operating as{' '}
          <strong data-testid="packet-effective-call">
            {effectiveCall(baseCall, ssid)}
          </strong>
        </p>
      </section>

      {intent === 'p2p' && (
        <section className="radio-panel-sec">
          <h5>Listen</h5>
          <button
            type="button"
            className={`radio-panel-btn ${armed ? 'radio-panel-btn-bad' : 'radio-panel-btn-primary'}`}
            data-testid="packet-listen-btn"
            aria-pressed={armed}
            onClick={onListen}
          >
            {armed
              ? `Waiting for a call as ${effectiveCall(baseCall, ssid)} — Stop`
              : 'Listen for an incoming call'}
          </button>
          {/* listenDefault is a PREFERENCE (auto-arm on startup), distinct
              from the live armed state above — it does not imply live
              listening. Restored 2026-05-31 from the legacy
              PacketConnectionPanel after Codex P1 flagged it as a lost
              feature on this PR. */}
          <label
            className="packet-listen-pref"
            data-testid="listen-default-pref"
            style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 8, fontSize: 12, color: 'var(--text-faint, #94a3b8)' }}
          >
            <input
              type="checkbox"
              data-testid="listen-default-checkbox"
              checked={listenDefault}
              onChange={onToggleListenDefault}
            />
            Auto-arm Listen at startup
          </label>
        </section>
      )}

      <section className="radio-panel-sec">
        <h5>Connect</h5>
        <label className="radio-panel-input-row">
          <span>To</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="packet-target-input"
            placeholder="call sign (gateway or peer)"
            value={target}
            spellCheck={false}
            autoCapitalize="characters"
            autoCorrect="off"
            onChange={(e) => setTarget(e.target.value)}
          />
        </label>

        <div data-testid="packet-relays">
          {relays.map((r, i) => (
            <label key={i} className="radio-panel-input-row">
              <span>{`Relay ${i + 1}`}</span>
              <span style={{ display: 'flex', gap: 4 }}>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid={`packet-relay-${i}`}
                  placeholder="W7RPT-1"
                  value={r}
                  spellCheck={false}
                  autoCapitalize="characters"
                  autoCorrect="off"
                  onChange={(e) => onRelayChange(i, e.target.value)}
                />
                <button
                  type="button"
                  className="radio-panel-chip"
                  data-testid={`packet-relay-remove-${i}`}
                  aria-label={`Remove relay ${i + 1}`}
                  onClick={() => onRemoveRelay(i)}
                >
                  ✕
                </button>
              </span>
            </label>
          ))}
          {relays.length < 2 && (
            <button
              type="button"
              className="radio-panel-chip"
              data-testid="packet-add-relay"
              onClick={onAddRelay}
            >
              + add relay (0–2)
            </button>
          )}
        </div>

        <p className="radio-panel-mono" data-testid="packet-path-preview">
          Path: <code>{pathPreview(baseCall, ssid, relays, target)}</code>
        </p>

        {/* Vocab unification (operator smoke 2026-05-31): use Start (idle) /
            Stop (active) to match the Telnet + ARDOP panels. The dedicated
            data-testid retains a stable hook for tests + grep. Listen is a
            distinct action that stays separate (it's the "armed" state). */}
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          data-testid="packet-start-btn"
          onClick={onConnect}
        >
          {target.trim() ? `Start (call ${target.trim()})` : 'Start'}
        </button>
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />
    </RadioPanel>
  );
}
