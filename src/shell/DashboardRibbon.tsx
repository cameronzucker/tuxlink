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
import { formatGridClock } from './gridClock';
import { DEV_FIXTURE, DEV_POSITION, DEV_CONNECTION_DASH } from '../mailbox/devFixture';
import { formatPacketConnection, type PacketUiState } from '../packet/packetStatus';
import { IdentitySwitcher } from './IdentitySwitcher';
import type { ActiveIdentityDto, IdentityListDto } from './identityTypes';

/**
 * Self-contained clock cell (tuxlink-sndh). Lives in its own subtree so the
 * 1-second tick only repaints the clock's two text nodes — not the entire
 * dashboard ribbon (which holds GridEdit, the SSID picker, the connect
 * controls, and the packet conn label, none of which depend on `now`).
 */
function ClockCell({ grid }: { grid: string | null }) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  const { utc, local, localTitle, source } = formatGridClock(now, grid);
  return (
    <div
      className="dash-value"
      data-testid="ribbon-time"
      data-time-source={source}
      title={localTitle}
    >
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
  /** Active non-packet radio transport (ARDOP/VARA) override for the connection
   *  label, supplied by AppShell when the last-active transport is a radio modem
   *  in the idle/disconnected window. Lower precedence than `packet`. */
  radioConn?: { label: string; tone: StatusTone } | null;
  /** Current "review pending inbound before download" preference (tuxlink-pmp5),
   *  or null before config_read resolves. Drives the inline "On connect" control:
   *  null/true ⇒ Review is active (the default); false ⇒ Download all. */
  reviewInbound?: boolean | null;
  /** Persist the review-inbound choice (AppShell calls config_set_review_inbound).
   *  When omitted, the "On connect" control is not rendered — keeps prop-free
   *  consumers/tests unchanged. */
  onReviewInboundChange?: (enabled: boolean) => void;
  /** APRS tactical-chat status control (entry ①). Absent → the control is not
   *  rendered. `unread` drives the badge; `onOpen` brings chat into the dock.
   *  `onToggleListening` (when provided) renders a slider switch that starts/stops
   *  the APRS listener (off⇄on) — the status bar is the operator's settled home
   *  for the APRS on/off control. `toggleBusy` disables the switch while a
   *  start/stop is in flight. `listening` reflects BACKEND TRUTH (the switch
   *  never optimistically flips). */
  aprs?: {
    listening: boolean;
    unread: number;
    onOpen: () => void;
    onToggleListening?: () => void;
    toggleBusy?: boolean;
  };
  /** Phase 7 (tuxlink-noa0): the full identity list for the inline switcher
   *  dropdown, or null while loading. Only consulted when `onSwitchIdentity`
   *  is also provided (the switcher branch). */
  identities?: IdentityListDto | null;
  /** The active identity session for the closed-chip label, or null pre-auth.
   *  Only consulted in the switcher branch. */
  activeIdentity?: ActiveIdentityDto | null;
  /** Authenticate + switch identity (the switch action). When provided, the
   *  callsign slot renders the IdentitySwitcher in place of the bare callsign
   *  row; when omitted, the legacy bare markup renders (back-compat — keeps
   *  prop-free consumers/tests unchanged). */
  onSwitchIdentity?: (args: { callsign: string; credential: string; tacticalLabel: string | null }) => Promise<void>;
}

// tuxlink-djnl: React.memo so 2s status-poll renders (now reference-stable
// via useStatusData's useMemo) and other shell-level renders skip the ribbon
// when its props haven't changed. The 1s clock tick already lives inside the
// scoped ClockCell subtree, so a memo'd ribbon stays still while time advances.
export const DashboardRibbon = memo(function DashboardRibbon({ data, onConnect, connecting, onAbort, packet, radioConn, reviewInbound, onReviewInboundChange, aprs, identities, activeIdentity, onSwitchIdentity }: DashboardRibbonProps) {
  const { callsign, grid, state, connection: connectionFromData } = data;
  // Task 14 (tuxlink-c79g, spec §4.3 + Codex P1 #4): after a grid commit or a
  // source flip resolves, invalidate the config_read query so the source chip
  // + grid value refresh within one render cycle instead of waiting up to 5s
  // for the next config poll. Local optimistic state via useState was rejected
  // because two sources of truth risk divergence on error paths.
  const queryClient = useQueryClient();
  // bd-tuxlink-y8tf: the ribbon shows the bare callsign only. AX.25 SSID is set
  // per-transport in the Packet/APRS panes, not on the ribbon identity chip.
  const displayCall = callsign;
  // Position (GPS coords) is a deferred data source; the dev fixture shows the mock
  // value, and the real app omits the item until GPS exists.
  const position = DEV_FIXTURE ? DEV_POSITION : null;
  // connection string is pre-formatted by useStatusData via formatConnectionState,
  // so it always names the real configured/active transport (tuxlink-989 fix).
  const connection = DEV_FIXTURE ? DEV_CONNECTION_DASH : connectionFromData;

  // Transport override: when an active radio transport is selected, replace the
  // connection label + tone so the ribbon reflects the last-active modem rather
  // than the generic config label. Packet takes precedence over the ARDOP/VARA
  // override (they are mutually exclusive in practice, but ordering is explicit).
  const packetConn = packet ? formatPacketConnection(packet) : null;
  const transportConn = packetConn ?? radioConn ?? null;
  const connectionLabel = transportConn ? transportConn.label : connection;
  const connectionTone = transportConn ? transportConn.tone : state.tone;

  return (
    <div className="dashboard" data-testid="dashboard-ribbon" role="banner">
      <div className="dash-item">
        <div className="dash-label">Callsign</div>
        {/* Phase 7 (tuxlink-noa0): when the identity-switch handler is wired
            (production AppShell path), render the IdentitySwitcher AS the
            callsign cell. It already renders the `.dash-callsign-row` +
            `data-testid="ribbon-callsign"` container, so it must NOT be wrapped
            in another `ribbon-callsign` (that would duplicate the testid). When
            the handler is omitted (prop-free consumers / unit tests), the legacy
            bare markup below renders the callsign. (bd-tuxlink-y8tf: the SSID
            select was removed from this chip — SSID is per-transport now.) */}
        {onSwitchIdentity ? (
          <IdentitySwitcher
            active={activeIdentity ?? null}
            list={identities ?? null}
            onSwitch={onSwitchIdentity}
          />
        ) : (
        <div className="dash-value callsign dash-callsign-row" data-testid="ribbon-callsign">
          {/* Legacy bare markup (prop-free consumers / unit tests with no
              onSwitchIdentity handler): plain callsign text, no SSID. Empty
              string before a callsign exists keeps the "no dangling dash"
              behavior. */}
          <span className="dash-callsign-text" data-testid="ribbon-callsign-text">
            {displayCall}
          </span>
        </div>
        )}
      </div>
      <div className="dash-divider" />

      <div className="dash-item dash-item--grid">
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
        <ClockCell grid={grid} />
      </div>
      <div className="dash-divider" />

      <div className="dash-item dash-item--connection">
        <div className="dash-label">Connection</div>
        <div
          className="dash-value dash-connection"
          data-testid="ribbon-connection"
          title={typeof connectionLabel === 'string' ? connectionLabel : undefined}
        >
          <span className={`dash-status-dot ${dashDotClass(connectionTone)}`} aria-hidden="true" />
          <span className="dash-connection-text">{connectionLabel}</span>
        </div>
      </div>

      {onReviewInboundChange && (
        <>
          <div className="dash-divider" />
          <div className="dash-item">
            <div className="dash-label">On connect</div>
            <div
              className="seg"
              role="group"
              aria-label="On connect: review pending messages, or download all"
              data-testid="ribbon-review-inbound"
            >
              <button
                type="button"
                className={reviewInbound !== false ? 'active' : ''}
                aria-pressed={reviewInbound !== false}
                onClick={() => onReviewInboundChange(true)}
                data-testid="review-inbound-review"
              >
                Review
              </button>
              <button
                type="button"
                className={reviewInbound === false ? 'active' : ''}
                aria-pressed={reviewInbound === false}
                onClick={() => onReviewInboundChange(false)}
                data-testid="review-inbound-download-all"
              >
                Download all
              </button>
            </div>
          </div>
        </>
      )}

      {aprs && (
        <>
          <div className="dash-divider" />
          <div className="dash-item dash-aprs">
            <div className="dash-label">APRS</div>
            {/* One control, mirroring the Connection item's dot+label pattern (not
                a bespoke slider — that was the lone archetype that read "too
                different" AND split the click target ambiguously). Clicking it
                starts/stops APRS listening and opens the APRS panel; the dot is
                BACKEND TRUTH (green=listening, amber=connecting, faint=off).
                Starting needs a configured radio — when none is set up the panel
                opens to its radio picker so the operator can set one up, instead
                of the control silently refusing. Falls back to onOpen for
                prop-light consumers/tests that don't pass onToggleListening. */}
            <button
              type="button"
              className="dash-aprs-control"
              data-testid="dash-aprs-control"
              aria-pressed={aprs.listening}
              disabled={aprs.toggleBusy}
              onClick={aprs.onToggleListening ?? aprs.onOpen}
              title={
                aprs.listening
                  ? 'APRS listening — click to stop'
                  : aprs.toggleBusy
                    ? 'Starting APRS…'
                    : 'Start APRS listening — opens the APRS panel (set up a radio there if none is configured)'
              }
            >
              <span
                className={`dash-status-dot ${aprs.listening ? '' : aprs.toggleBusy ? 'connecting' : 'idle'}`}
                aria-hidden="true"
              />
              <span className="dash-aprs-state">
                {aprs.listening ? 'Listening' : aprs.toggleBusy ? 'Connecting…' : 'Off'}
              </span>
              {aprs.unread > 0 && (
                <span className="dash-aprs-unread" data-testid="dash-aprs-unread">{aprs.unread}</span>
              )}
            </button>
          </div>
        </>
      )}

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
