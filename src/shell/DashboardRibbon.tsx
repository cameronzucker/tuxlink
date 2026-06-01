/**
 * DashboardRibbon — top operator-info bar (Mock B `.dashboard`).
 *
 * Callsign · Grid · Position (GPS) · UTC/Local · Connection. The approved Mock B
 * keeps these always-visible up top (the emcomm operator's at-a-glance state).
 * Styling lives in AppShell.css (`.layout-b .dashboard`). Data comes from the
 * shared `useStatusData` poll (passed in by AppShell); the live clock is local.
 */

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { StatusBarData, StatusTone } from './useStatus';
import { GridEdit } from './GridEdit';
import { DEV_FIXTURE, DEV_POSITION, DEV_CONNECTION_DASH } from '../mailbox/devFixture';
import { formatPacketConnection, type PacketUiState } from '../packet/packetStatus';
import { effectiveCall as renderEffectiveCall, ssidOptions } from '../packet/packetConfig';

function useClock() {
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
  return { utc, local };
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

export function DashboardRibbon({ data, onConnect, connecting, onAbort, packet, ssid, onSsidChange }: DashboardRibbonProps) {
  const { utc, local } = useClock();
  const { callsign, grid, state, connection: connectionFromData } = data;
  // Compose the operator's effective AX.25 call (base-SSID) when an SSID is
  // available. When ssid is undefined (no packet config loaded), fall back to
  // the bare callsign so we don't render a misleading "-0" before load.
  const displayCall = ssid !== undefined && callsign
    ? renderEffectiveCall(callsign, ssid)
    : callsign;
  // Position (GPS coords) is a v0.1 data source; the dev fixture shows the mock
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
          <span className="dash-callsign-text">{displayCall}</span>
          {onSsidChange && (
            // Inline SSID selector — operator smoke 2026-05-31. The dropdown
            // reads tiny so it doesn't dominate the ribbon, but it's a real
            // <select> (keyboard-friendly, screen-reader-friendly) rather
            // than a bespoke chip menu. value=0..15.
            <select
              className="dash-ssid-select"
              data-testid="ribbon-ssid-select"
              aria-label="AX.25 SSID"
              title="AX.25 SSID — same value used by the Packet panel"
              value={ssid ?? 0}
              onChange={(e) => onSsidChange(Number(e.target.value))}
            >
              {/* Bare integer (no leading dash) — operator smoke 2026-05-31.
                  The callsign already renders as `<base>-<ssid>` (e.g. W7CPZ-7)
                  via renderEffectiveCall above; prefixing the option with `-`
                  duplicated the dash visually (`W7CPZ-7  -7`). PacketRadioPanel's
                  SSID select keeps the `-N` form because there it sits in a
                  labeled SSID row where the leading dash is read as a value
                  hint, not as a duplicate of an adjacent callsign suffix. */}
              {ssidOptions().map((n) => (
                <option key={n} value={n}>{n.toString()}</option>
              ))}
            </select>
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
          onCommit={(g) => invoke('config_set_grid', { grid: g })}
          onUseGps={() => invoke('position_set_source', { source: 'Gps' })}
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
        <div className="dash-value" data-testid="ribbon-time">
          {utc} · {local}
        </div>
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
}
