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

import { useCallback, useEffect, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { ModemLinkSection, type ModemLinkFields } from '../sections/ModemLinkSection';
import { ManagedModemSection } from '../sections/ManagedModemSection';
import { AllowedStationsEditor } from '../sections/AllowedStationsEditor';
import { useListenerState } from '../sections/useListenerState';
import { effectiveCall, pathPreview, ssidOptions } from '../../packet/packetConfig';
import type { PacketConfigDto, StableAudioId, PttChoice } from '../../packet/packetTypes';
import { FavoritesTabs } from '../../favorites/FavoritesTabs';
import { useFavorites } from '../../favorites/useFavorites';
import { listenGatewayPrefill } from '../../favorites/prefillEvent';
import { tsLocal } from '../../favorites/ts-local';
import type { FavoriteDial } from '../../favorites/types';
import '../sections/ListenSection.css';

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
  /** tuxlink-6jpf: open the station finder ("Find a gateway") from the panel. */
  onFindGateway?: () => void;
}

export function PacketRadioPanel({ intent, baseCall, onClose, onFindGateway }: PacketRadioPanelProps) {
  const [config, setConfig] = useState<PacketConfigDto | null>(null);
  const [target, setTarget] = useState('');
  const [relays, setRelays] = useState<string[]>([]);
  const [armed, setArmed] = useState(false);
  // listenDefault PREFERENCE (auto-arm on startup) — distinct from the
  // live `armed` state above. Synced from config on load + persisted via
  // packet_set_listen. Restored 2026-05-31 from legacy PacketConnectionPanel.
  const [listenDefault, setListenDefault] = useState<boolean>(true);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Favorites integration (Task B6-PACKET). RADIO-1: a favorite's Connect is
  // PRE-FILL ONLY — it sets `target` via `handlePrefill` and NEVER invokes
  // `packet_connect`. The operator's later Start click is the Part 97 consent
  // gate. recordAttempt logs the HONEST on-air outcome: packet_connect is a
  // BLOCKING connect→B2F, so `reached` is recorded on resolve and `failed` in
  // the catch (no status-transition watching needed — the single call's
  // resolve/reject IS the signal).
  const { recordAttempt } = useFavorites('packet');
  // The favorite whose Connect was last clicked. Carries its metadata into the
  // connection record IFF its gateway matches the connect target. Cleared on a
  // manual target edit (a hand-typed target is not the prefilled favorite).
  const pendingDialRef = useRef<FavoriteDial | null>(null);
  const handlePrefill = useCallback((dial: FavoriteDial) => {
    setTarget(dial.gateway);
    pendingDialRef.current = dial;
  }, []);

  useEffect(
    () => listenGatewayPrefill('packet', handlePrefill),
    [handlePrefill],
  );
  // Build the dial for a connection record. The gateway is the connect target.
  // If the prefilled favorite matches it (case-insensitive), carry its metadata
  // (band/grid/note) into the record; otherwise record a minimal manual dial.
  // (Packet favorites do NOT carry relay chains — a known forward gap; prefill
  // sets only the target callsign.)
  const buildRecordDial = (call: string): FavoriteDial => {
    const gw = call.trim();
    const pend = pendingDialRef.current;
    if (pend && pend.gateway.trim().toUpperCase() === gw.toUpperCase()) {
      return { ...pend, mode: 'packet', gateway: gw };
    }
    return { mode: 'packet', gateway: gw };
  };

  // Packet listener allowlist plumbing (spec §1.3). Packet has no
  // IP-pattern layer (AX.25 isn't IP-routed); we pass undefined for
  // IP handlers so the AllowedStationsEditor hides that row entirely.
  // The Packet `*_set_allow_all` arg-key is `allow_all` (snake-case,
  // matches the backend handler signature in packet_commands.rs); the
  // add/remove handlers take `callsign`.
  const packetListener = useListenerState({
    commands: {
      listen: 'packet_listen',
      setListen: 'packet_set_listen',
      allowedGet: 'packet_allowed_stations_get',
      allowedAddCallsign: 'packet_allowed_stations_add',
      allowedAddCallsignArgKey: 'callsign',
      allowedRemoveCallsign: 'packet_allowed_stations_remove',
      allowedRemoveCallsignArgKey: 'callsign',
      allowedSetAllowAll: 'packet_allowed_stations_set_allow_all',
      // Tauri auto-camelCases Rust arg `allow_all: bool` → JS wire key `allowAll`.
      // Codex review 2026-06-03 [P2] (tuxlink-7vea): the prior `allow_all` key
      // here meant Tauri delivered no value to the Rust handler.
      allowedSetAllowAllArgKey: 'allowAll',
    },
  });

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

  // Connection mode: 'managed' (recommended accessibility path — pick a sound
  // card + PTT, no .conf authoring) vs 'byo' (bring your own KISS endpoint —
  // the Tcp/Serial/Bluetooth editor). Derived from the persisted linkKind so we
  // don't clobber an operator who already configured a Tcp/Serial/Bluetooth
  // link: those three select BYO; 'Managed' or unconfigured (null) selects
  // Managed (the default for a fresh panel). Operator switches are sticky for
  // the session via this state; the persisted linkKind only changes when they
  // actually pick a device (Managed) or edit the modem link (BYO).
  const isByoKind = (k: PacketConfigDto['linkKind']): boolean =>
    k === 'Tcp' || k === 'Serial' || k === 'Bluetooth' || k === 'UvproNative';
  const [connectionMode, setConnectionMode] = useState<'managed' | 'byo'>('managed');
  // Seed the connection mode from config once it loads (a fresh config with no
  // link → Managed; an existing Tcp/Serial/Bluetooth link → BYO so we don't
  // override the operator's prior choice). Keyed on the linkKind only.
  const seededRef = useRef(false);
  useEffect(() => {
    if (!config || seededRef.current) return;
    seededRef.current = true;
    setConnectionMode(isByoKind(config.linkKind) ? 'byo' : 'managed');
  }, [config]);

  // Persist a managed device + PTT choice: linkKind:'Managed' + the structured
  // managedAudioDevice / managedPtt. Clears the BYO scalar fields so a stale
  // Tcp host / serial device doesn't ride along on the Managed DTO.
  const onManagedChange = (device: StableAudioId, ptt: PttChoice) => {
    if (!config) return;
    persistDto({
      ...config,
      linkKind: 'Managed',
      tcpHost: null,
      tcpPort: null,
      serialDevice: null,
      serialBaud: null,
      btMac: null,
      managedAudioDevice: device,
      managedPtt: ptt,
    });
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

  // H4: onConnect is async/await so a packet_connect failure is observable —
  // the prior fire-and-forget `.catch(() => {})` swallowed every rejection,
  // making a failed connect indistinguishable from a successful one. Now the
  // single BLOCKING connect→B2F call's resolve records `reached`; its reject
  // records `failed` in the CATCH (never the finally), so a pre-air guard never
  // logs a spurious gateway failure.
  const onConnect = async () => {
    const call = target.trim();
    if (!call) return; // pre-air guard: precedes the dial/record, so an empty-target click records nothing
    const path = relays.map((r) => r.trim()).filter(Boolean);
    const dial = buildRecordDial(call);
    try {
      await invoke('packet_connect', { call, path });
      void recordAttempt(dial, 'reached', tsLocal()); // blocking connect→B2F resolved = honest reach
    } catch {
      void recordAttempt(dial, 'failed', tsLocal()); // record failed in the catch (NOT finally)
    }
  };

  const headerSub = config
    ? config.linkKind === 'Tcp'
      ? `${config.tcpHost ?? '127.0.0.1'}:${config.tcpPort ?? 8001}`
      : config.linkKind === 'Bluetooth'
      ? `BT ${config.btMac ?? '(no device)'}`
      : config.linkKind === 'UvproNative'
      ? `UV-Pro ${config.btMac ?? '(no device)'}`
      : config.linkKind === 'Managed'
      ? `Managed ${config.managedAudioDevice?.value ?? '(no sound card)'}`
      : config.linkKind === 'Serial'
      ? `${config.serialDevice ?? '(no device)'}`
      : undefined
    : undefined;

  return (
    <RadioPanel
      mode={{ kind: 'packet', intent }}
      state="disconnected"
      sub={headerSub}
      onClose={onClose}
      onFindGateway={onFindGateway}
    >
      <section className="radio-panel-sec">
        <h5>Connection</h5>
        <div
          className="radio-panel-segmented"
          role="group"
          aria-label="Connection type"
          data-testid="packet-connection-mode"
        >
          <button
            type="button"
            className={connectionMode === 'managed' ? 'active' : ''}
            aria-pressed={connectionMode === 'managed'}
            data-testid="packet-conn-managed"
            onClick={() => setConnectionMode('managed')}
          >
            Managed (recommended)
          </button>
          <button
            type="button"
            className={connectionMode === 'byo' ? 'active' : ''}
            aria-pressed={connectionMode === 'byo'}
            data-testid="packet-conn-byo"
            onClick={() => setConnectionMode('byo')}
          >
            Bring your own KISS endpoint
          </button>
        </div>
        <p className="radio-panel-mono" data-testid="packet-conn-hint">
          {connectionMode === 'managed'
            ? 'tuxlink runs the modem for you — pick a sound card and PTT line.'
            : 'Connect tuxlink to a KISS TNC you run (TCP, USB, or Bluetooth).'}
        </p>
      </section>

      {connectionMode === 'managed' ? (
        <ManagedModemSection
          audioDevice={config?.managedAudioDevice ?? null}
          ptt={config?.managedPtt ?? null}
          effectiveCall={effectiveCall(baseCall, ssid)}
          onChange={onManagedChange}
        />
      ) : (
        <ModemLinkSection
          kind={
            config?.linkKind === 'Bluetooth'
              ? 'Bluetooth'
              : config?.linkKind === 'UvproNative'
              ? 'UvproNative'
              : config?.linkKind === 'Serial'
              ? 'Serial'
              : 'Tcp'
          }
          host={config?.tcpHost ?? undefined}
          port={config?.tcpPort ?? undefined}
          serialDevice={config?.serialDevice ?? undefined}
          serialBaud={config?.serialBaud ?? undefined}
          btMac={config?.btMac ?? undefined}
          allowUvproNative
          onChange={onLinkChange}
        />
      )}

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

          {/* Allowed stations expander — spec §1.3. AX.25 has no IP
              layer, so the editor's IP row is hidden. The summary's
              count chip surfaces the current list shape so the
              operator can see whether they have a curated allowlist
              without expanding the accordion. */}
          <details className="expander" data-testid="packet-allowed-expander">
            <summary className="expander-summary">
              Allowed stations
              <span
                className="expander-count"
                data-testid="packet-allowed-count"
              >
                {packetListener.allowed.allowAll
                  ? 'allow any'
                  : packetListener.allowed.callsigns.length === 0
                  ? 'restrict to none'
                  : `${packetListener.allowed.callsigns.length} callsign${packetListener.allowed.callsigns.length === 1 ? '' : 's'}`}
              </span>
            </summary>
            <AllowedStationsEditor
              allowAll={packetListener.allowed.allowAll}
              callsigns={packetListener.allowed.callsigns}
              helpText="Inbound peers whose AX.25 source callsign matches the list are admitted. AX.25 has no IP layer, so callsign matching is the only application-layer gate."
              onSetAllowAll={packetListener.setAllowAll}
              onAddCallsign={packetListener.addCallsign}
              onRemoveCallsign={packetListener.removeCallsign}
              testIdPrefix="packet-allowed"
            />
          </details>
        </section>
      )}

      <section className="radio-panel-sec">
        <h5>Connect</h5>
        {/* Connect-target surface — Favorites / Recent / Manual (Task
            B6-PACKET). The hand-entry To + relays + path-preview fields are the
            Manual tab's content; a favorite's Connect PRE-FILLS the target via
            handlePrefill and never transmits (RADIO-1). The Start button stays
            OUTSIDE the tabs (rendered after, still inside this section) so it
            remains visible regardless of the active tab — mirrors ARDOP keeping
            its action buttons outside the tabs. */}
        <FavoritesTabs
          mode="packet"
          onPrefill={handlePrefill}
          manualContent={
            <>
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
                  onChange={(e) => {
                    setTarget(e.target.value);
                    // A hand-typed target is not the prefilled favorite — drop
                    // the association so the record doesn't carry stale metadata.
                    pendingDialRef.current = null;
                  }}
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
            </>
          }
        />

        {/* Vocab unification (operator smoke 2026-05-31): use Start (idle) /
            Stop (active) to match the Telnet + ARDOP panels. The dedicated
            data-testid retains a stable hook for tests + grep. Listen is a
            distinct action that stays separate (it's the "armed" state).
            Kept OUTSIDE FavoritesTabs so it stays visible on every tab. */}
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
