/**
 * DashboardRibbon — top operator-info bar (Mock B `.dashboard`).
 *
 * Callsign · Grid · Position (GPS) · UTC/Local · Connection. The approved Mock B
 * keeps these always-visible up top (the emcomm operator's at-a-glance state).
 * Styling lives in AppShell.css (`.layout-b .dashboard`). Data comes from the
 * shared `useStatusData` poll (passed in by AppShell); the live clock is local.
 */

import { memo, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQueryClient } from '@tanstack/react-query';
import type { StatusBarData, StatusTone } from './useStatus';
import { GridEdit } from './GridEdit';
import { DEV_FIXTURE, DEV_POSITION, DEV_CONNECTION_DASH } from '../mailbox/devFixture';
import { formatPacketConnection, type PacketUiState } from '../packet/packetStatus';
import { effectiveCall as renderEffectiveCall, ssidOptions } from '../packet/packetConfig';

/**
 * Self-contained clock cell (tuxlink-sndh). Lives in its own subtree so the
 * 1-second tick only repaints the clock's two text nodes — not the entire
 * dashboard ribbon (which holds GridEdit, the SSID picker, the connect
 * controls, and the packet conn label, none of which depend on `now`).
 */
function ClockCell() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  const utc = now.toISOString().substring(11, 16) + 'z';
  const local = now.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    timeZoneName: 'short',
  });
  return (
    <div className="dash-value" data-testid="ribbon-time">
      {utc} · {local}
    </div>
  );
}

/** Map a status tone to the mock's dash-status-dot variant. */
function dashDotClass(tone: StatusTone): string {
  switch (tone) {
    case 'idle':
      return 'idle';
    case 'good':
      return ''; // default green
    case 'warn':
      return 'connecting';
    case 'error':
      return 'tx';
  }
}

export interface DashboardRibbonProps {
  data: StatusBarData;
  /** Trigger a CMS connection (send outbox + receive). When omitted, the
   *  Connect control is not rendered (keeps the ribbon's unit tests prop-free). */
  onConnect?: () => void;
  /** True while a connection is in progress (disables the button + shows a
   *  "Connecting…" label). The result/error is surfaced in the session log,
   *  not beside the button. */
  connecting?: boolean;
  /** Cancel an in-flight connection (tuxlink-9z2). The Abort control is rendered
   *  only while `connecting`; it shuts the connecting socket so a slow TLS/login/
   *  exchange phase unblocks, returning the backend to Disconnected. */
  onAbort?: () => void;
  /** Packet transport state; when active, overrides the CMS connection label. */
  packet?: PacketUiState;
  /** Effective AX.25 SSID (0..15) for the callsign chip. Undefined when no
   *  packet config has loaded — the callsign renders without a -N suffix. */
  ssid?: number;
  /** Persist a new SSID. Operator smoke 2026-05-31: SSID is editable inline
   *  from the dashboard ribbon (not just the PacketRadioPanel) so the operator
   *  doesn't need to open the radio panel to switch. */
  onSsidChange?: (n: number) => void;
}

// tuxlink-djnl: React.memo so 2s status-poll renders (now reference-stable
// via useStatusData's useMemo) and other shell-level renders skip the ribbon
// when its props haven't changed. The 1s clock tick already lives inside the
// scoped ClockCell subtree, so a memo'd ribbon stays still while time advances.
export const DashboardRibbon = memo(function DashboardRibbon({ data, onConnect, connecting, onAbort, packet, ssid, onSsidChange }: DashboardRibbonProps) {
  const { callsign, grid, state, connection: connectionFromData } = data;
  // Task 14 (tuxlink-c79g, spec §4.3 + Codex P1 #4): after a grid commit or a
  // source flip resolves, invalidate the config_read query so the source chip
  // + grid value refresh within one render cycle instead of waiting up to 5s
  // for the next config poll. Local optimistic state via useState was rejected
  // because two sources of truth risk divergence on error paths.
  const queryClient = useQueryClient();
  // Non-editable fallback (no onSsidChange handler — pre-wizard / external
  // consumers): show the effective `<base>-<N>` so the operator still sees
  // their AX.25 call. The editable path below splits callsign + SSID into
  // two surfaces; the picker owns the `-N` display in that case.
  const displayCall = ssid !== undefined && callsign
    ? renderEffectiveCall(callsign, ssid)
    : callsign;
  // Position (GPS coords) is a deferred data source; the dev fixture shows the mock
  // value, and the real app omits the item until GPS exists.
  const position = DEV_FIXTURE ? DEV_POSITION : null;
  // connection string is pre-formatted by useStatusData via formatConnectionState,
  // so it always names the real configured/active transport (tuxlink-989 fix).
  const connection = DEV_FIXTURE ? DEV_CONNECTION_DASH : connectionFromData;

  // Packet override: when packet is active, replace the connection label + tone.
  const packetConn = packet ? formatPacketConnection(packet) : null;
  const connectionLabel = packetConn ? packetConn.label : connection;
  const connectionTone = packetConn ? packetConn.tone : state.tone;

  return (
    <div className="dashboard" data-testid="dashboard-ribbon" role="banner">
      <div className="dash-item">
        <div className="dash-label">Callsign</div>
        <div className="dash-value callsign dash-callsign-row" data-testid="ribbon-callsign">
          {/* Operator smoke 2026-05-31 (round 4 — tuxlink-i63g): the round-3
              "one select with full `<base>-<N>` option labels" approach was
              rejected. Two issues with it: (a) operator wanted the picker to
              be just the SSID (`-N`), not the whole call; (b) at 2-digit
              SSIDs (`-10` .. `-15`) the OS scroll bar in the open popup
              visually covered the second digit so the operator could only
              read `-1` for `-10`. Restored pattern: bare callsign text chip
              + adjacent narrow picker whose options are just `-N`. The
              picker has an explicit min-width (AppShell.css) wide enough
              that the popup gutter does not overlap option text. The
              callsign chip itself NEVER carries an SSID suffix in the
              editable branch — that would put `-N` on two surfaces again.

              Fallback (no callsign yet, pre-wizard / pre-identity): render
              empty string, matching the prior "no dangling dash" behavior.
              Fallback (no onSsidChange handler): plain text span showing
              the effective call so external consumers still see `<base>-<N>`. */}
          {callsign && onSsidChange ? (
            <>
              <span className="dash-callsign-text" data-testid="ribbon-callsign-text">
                {callsign}
              </span>
              <select
                className="dash-callsign-select dash-ssid-select"
                data-testid="ribbon-ssid-select"
                aria-label="AX.25 SSID"
                title="Click to switch AX.25 SSID"
                value={ssid ?? 0}
                onChange={(e) => onSsidChange(Number(e.target.value))}
              >
                {ssidOptions().map((n) => (
                  <option key={n} value={n}>{`-${n}`}</option>
                ))}
              </select>
            </>
          ) : (
            <span className="dash-callsign-text">{displayCall}</span>
          )}
        </div>
      </div>
      <div className="dash-divider" />

      <div className="dash-item">
        <div className="dash-label">Grid</div>
        <GridEdit
          grid={grid}
          source={data.position_source}
          gpsReady={data.gpsReady ?? false}
          onCommit={async (g) => {
            await invoke('config_set_grid', { grid: g });
            queryClient.invalidateQueries({ queryKey: ['config_read'] });
          }}
          onUseGps={async () => {
            await invoke('position_set_source', { source: 'Gps' });
            queryClient.invalidateQueries({ queryKey: ['config_read'] });
          }}
          /* tuxlink-z5pz (spec §4.1 amended): the MANUAL segment's click in
             the source segmented control fires onUseManual + enters edit
             mode. The DashboardRibbon-side handler is a no-op — the actual
             work happens inside GridEdit (enterEdit() opens the grid input;
             the operator's Enter-commit fires the existing onCommit path
             above, which is the T4 config_set_grid that atomically persists
             cfg.privacy.position_source = Manual + the new grid value).
             onUseManual is wired explicitly (Choice B per the spec §4.1
             prop-shape directive) so DashboardRibbon retains a test-spy
             hook in case future optimistic-invalidate behavior is added
             here, mirroring the onUseGps shape. */
          onUseManual={() => {}}
        />
      </div>
      <div className="dash-divider" />

      {position && (
        <>
          <div className="dash-item">
            <div className="dash-label">Position</div>
            <div className="dash-value good" data-testid="ribbon-position">
              {position}
            </div>
          </div>
          <div className="dash-divider" />
        </>
      )}

      <div className="dash-item">
        <div className="dash-label">UTC / Local</div>
        <ClockCell />
      </div>
      <div className="dash-divider" />

      <div className="dash-item">
        <div className="dash-label">Connection</div>
        <div
          className="dash-value dash-connection"
          data-testid="ribbon-connection"
          title={typeof connectionLabel === 'string' ? connectionLabel : undefined}
        >
          <span className={`dash-status-dot ${dashDotClass(connectionTone)}`} aria-hidden="true" />
          {connectionLabel}
        </div>
      </div>

      {onConnect && (
        <>
          <div className="dash-divider" />
          <div className="dash-item dash-connect">
            <button
              type="button"
              className="connect-button"
              onClick={onConnect}
              disabled={connecting}
              data-testid="connect-button"
            >
              {connecting ? 'Connecting…' : 'Connect'}
            </button>
            {connecting && onAbort && (
              <button
                type="button"
                className="abort-button"
                onClick={onAbort}
                data-testid="abort-button"
              >
                Abort
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
});
