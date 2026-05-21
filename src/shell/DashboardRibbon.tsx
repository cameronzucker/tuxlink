/**
 * DashboardRibbon — top operator-info bar (Mock B `.dashboard`).
 *
 * Callsign · Grid · Position (GPS) · UTC/Local · Connection. The approved Mock B
 * keeps these always-visible up top (the emcomm operator's at-a-glance state).
 * Styling lives in AppShell.css (`.layout-b .dashboard`). Data comes from the
 * shared `useStatusData` poll (passed in by AppShell); the live clock is local.
 */

import { useEffect, useState } from 'react';
import type { StatusBarData, StatusTone } from './useStatus';
import { DEV_FIXTURE, DEV_POSITION, DEV_CONNECTION_DASH } from '../mailbox/devFixture';

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
}

export function DashboardRibbon({ data, onConnect, connecting }: DashboardRibbonProps) {
  const { utc, local } = useClock();
  const { callsign, grid, state, connection: connectionFromData } = data;
  // Position (GPS coords) is a v0.1 data source; the dev fixture shows the mock
  // value, and the real app omits the item until GPS exists.
  const position = DEV_FIXTURE ? DEV_POSITION : null;
  // connection string is pre-formatted by useStatusData via formatConnectionState,
  // so it always names the real configured/active transport (tuxlink-989 fix).
  const connection = DEV_FIXTURE ? DEV_CONNECTION_DASH : connectionFromData;

  return (
    <div className="dashboard" data-testid="dashboard-ribbon" role="banner">
      <div className="dash-item">
        <div className="dash-label">Callsign</div>
        <div className="dash-value callsign" data-testid="ribbon-callsign">
          {callsign}
        </div>
      </div>
      <div className="dash-divider" />

      <div className="dash-item">
        <div className="dash-label">Grid</div>
        <div className="dash-value" data-testid="ribbon-grid">
          {grid}
        </div>
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
        <div className="dash-value" data-testid="ribbon-connection">
          <span className={`dash-status-dot ${dashDotClass(state.tone)}`} aria-hidden="true" />
          {connection}
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
          </div>
        </>
      )}
    </div>
  );
}
