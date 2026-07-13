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
import {
  EGRESS_STATUS_DISARMED,
  formatEgressRemaining,
  type EgressStatusDto,
} from '../security/egressTypes';
import { Button } from '../controls';
import type { Ft8UiState } from '../ft8ui/ft8Types';

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

/** The 4 ribbon-visible FT-8 states (Task C2, plan tuxlink-b026z.4 §Ribbon). */
export type Ft8RibbonState = 'off' | 'starting' | 'listening' | 'blocked';

/**
 * The 9→4 ribbon-state map (explicit — the hook exposes 9 `Ft8UiState`
 * members, the ribbon shows 4). A switch with no `default` over the full
 * `Ft8UiState` union: TypeScript enforces exhaustiveness here, so a future
 * 10th member added to `Ft8UiState` is a compile error in THIS function, not
 * a silent gap that falls through to some arbitrary branch.
 */
export function ft8RibbonState(state: Ft8UiState): Ft8RibbonState {
  switch (state) {
    case 'off':
      return 'off';
    case 'transitional':
      return 'starting';
    case 'decoding':
    case 'waiting-first-slot':
    case 'band-dead':
      return 'listening';
    case 'needs-setup':
    case 'device-lost':
    case 'wedged':
    case 'yielded':
      return 'blocked';
  }
}

/**
 * Sub-label for the 'blocked' ribbon state (spec §Ribbon). Only meaningful
 * when `ft8RibbonState(state) === 'blocked'`; returns null for the other 5
 * raw states (the caller only renders this alongside a 'blocked' badge).
 */
export function ft8BlockedLabel(state: Ft8UiState): string | null {
  // Sentence-case, matching the chip's sibling labels ('Off' / 'Listening' /
  // 'Starting…') and the APRS control's 'Listening' / 'Off' (QA round-3
  // finding 4: lowercase 'paused' read as inconsistent with the theme).
  switch (state) {
    case 'needs-setup':
      return 'Needs setup';
    case 'device-lost':
      return 'Disconnected';
    case 'wedged':
      return 'Restart';
    case 'yielded':
      return 'Paused';
    default:
      return null;
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
  /** Station Intelligence (FT-8) listener status control (Task C2, plan
   *  tuxlink-b026z.4 §Ribbon). Mirrors the `aprs` shape above: a dot + state
   *  word rendered after the APRS block. `uiState` is the RAW 9-member value
   *  from `useFt8Listener().uiState.state` — DashboardRibbon owns the 9→4
   *  reduction (see `ft8RibbonState` below) so the mapping is testable
   *  directly off this prop, one uiState at a time. Absent → the control is
   *  not rendered (keeps prop-free consumers/tests unchanged). */
  ft8?: {
    /** One of the hook's 9 states — never pre-reduced by the caller. */
    uiState: Ft8UiState;
    /** `snapshot.band` for the listening caption (e.g. "20m"), or null/undefined
     *  before a snapshot exists. Only consulted when the reduced state is
     *  'listening'. */
    band?: string | null;
    /** Decodes/min for the listening caption (deriveBandActivity.stripStats),
     *  or null/undefined when there's no rate to show yet. Only consulted
     *  when the reduced state is 'listening'. */
    decodesPerMin?: number | null;
    /** Opens the Station Intelligence panel. A 'blocked' click ALWAYS calls
     *  this — never `onToggleListening` (spec §Ribbon: blocked needs the
     *  operator's attention in the panel, not a toggle attempt that would
     *  just fail again). */
    onOpen: () => void;
    /** Starts/stops the FT-8 listener (off→start, listening→stop). Falls back
     *  to `onOpen` for prop-light consumers/tests that don't pass it (mirrors
     *  the `aprs` control's `onToggleListening ?? onOpen` fallback). Never
     *  invoked while the reduced state is 'blocked' — see `onOpen` above. */
    onToggleListening?: () => void;
    /** True while a start/stop is in flight OR the raw uiState is
     *  'transitional'. Disables the control EXCEPT in the 'blocked' state,
     *  which must always stay clickable so the operator can reach the panel. */
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
  /** Live egress-grant snapshot for the merged Elmer × Agent-send chip. Only
   *  `status` is read here (the chip is display-only: it shows arm/taint state
   *  and opens the Elmer drawer). The arm/disarm/re-arm actions were relocated
   *  to the drawer header (see ElmerPane); `onArm`/`onDisarm` remain on the type
   *  so AppShell can pass one egress object, but the ribbon does not call them. */
  egress?: {
    status: EgressStatusDto;
    onArm: (durationSecs: number) => void;
    onDisarm: () => void;
    busy?: boolean;
    error?: string | null;
  };
  /** Toggle the Elmer assistant drawer. When omitted, the launcher is not
   *  rendered (keeps prop-free consumers/tests unchanged). Co-located with the
   *  Agent-send arm chip — the assistant + its send authority as one unit. */
  onOpenElmer?: () => void;
  /** Whether the Elmer drawer is currently open (drives the launcher's
   *  aria-pressed / active state). */
  elmerOpen?: boolean;
}

/**
 * Live countdown for the ribbon agent chip. Seeds from the polled remaining
 * seconds and ticks locally each second; re-seeds when a fresh poll changes the
 * value. Scoped so only this text node repaints (mirrors EgressArmControl's
 * CountdownCell, but with its own testid so the two never collide in the DOM).
 */
function ChipCountdown({ remainingSecs }: { remainingSecs: number }) {
  const [secs, setSecs] = useState(remainingSecs);
  useEffect(() => setSecs(remainingSecs), [remainingSecs]);
  useEffect(() => {
    const id = setInterval(() => setSecs((s) => (s > 0 ? s - 1 : 0)), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <span className="dash-elmer-agent-cd" data-testid="ribbon-elmer-countdown">
      · {formatEgressRemaining(secs)}
    </span>
  );
}

/**
 * ElmerAgentChip — the merged ribbon control (the Elmer launcher AND the
 * at-a-glance agent-send state, in one slot). The constant ✦ anchors Elmer's
 * identity; the label transforms with arm state. Click ALWAYS opens the Elmer
 * drawer — arm/disarm/re-arm live in the drawer header (relocated from this
 * ribbon), so this control is display-only and never disarms on close.
 *
 *   disarmed → "✦ Elmer"               (launcher; pressed when the drawer is open)
 *   armed    → "✦ ● Agent send · M:SS"  (live countdown)
 *   tainted  → "✦ ⚠ Agent send · LOCKED"
 */
function ElmerAgentChip({
  status,
  elmerOpen,
  onOpenElmer,
}: {
  status: EgressStatusDto;
  elmerOpen?: boolean;
  onOpenElmer: () => void;
}) {
  const { armed, armedRemainingSecs, tainted } = status;
  const mode = tainted ? 'locked' : armed ? 'armed' : 'disarmed';
  const title =
    mode === 'locked'
      ? 'Elmer — agent send LOCKED (open to re-arm)'
      : mode === 'armed'
        ? 'Elmer — agent send armed (open to manage)'
        : 'Elmer — AI assistant';
  return (
    <button
      type="button"
      className={`dash-elmer-agent dash-elmer-agent--${mode}${elmerOpen ? ' is-open' : ''}`}
      data-testid="ribbon-elmer-launcher"
      data-mode={mode}
      aria-pressed={!!elmerOpen}
      aria-label="Elmer assistant and agent send authority"
      title={title}
      onClick={onOpenElmer}
    >
      <span className="dash-elmer-agent-spark" aria-hidden="true">✦</span>
      {mode === 'disarmed' && <span className="dash-elmer-agent-label">Elmer</span>}
      {mode === 'armed' && (
        <>
          <span className="dash-status-dot" aria-hidden="true" />
          <span className="dash-elmer-agent-label">Agent send</span>
          <ChipCountdown remainingSecs={armedRemainingSecs} />
        </>
      )}
      {mode === 'locked' && (
        <>
          <span className="dash-elmer-agent-warn" aria-hidden="true">⚠</span>
          <span className="dash-elmer-agent-label">Agent send · LOCKED</span>
        </>
      )}
    </button>
  );
}

// tuxlink-djnl: React.memo so 2s status-poll renders (now reference-stable
// via useStatusData's useMemo) and other shell-level renders skip the ribbon
// when its props haven't changed. The 1s clock tick already lives inside the
// scoped ClockCell subtree, so a memo'd ribbon stays still while time advances.
export const DashboardRibbon = memo(function DashboardRibbon({ data, onConnect, connecting, onAbort, packet, radioConn, reviewInbound, onReviewInboundChange, aprs, ft8, identities, activeIdentity, onSwitchIdentity, egress, onOpenElmer, elmerOpen }: DashboardRibbonProps) {
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

      {/* Station Intelligence (FT-8) listener control (Task C2, plan
          tuxlink-b026z.4 §Ribbon). Rendered AFTER the APRS block, mirroring
          its dot+label pattern. `rstate` reduces the raw 9-member uiState to
          one of the 4 ribbon states; the 'blocked' click ALWAYS opens the
          panel (never a toggle), even if `toggleBusy` is set — the operator
          must always be able to reach the panel to fix a blocked listener. */}
      {ft8 && (() => {
        const rstate = ft8RibbonState(ft8.uiState);
        const blockedLabel = ft8BlockedLabel(ft8.uiState);
        const busy = rstate !== 'blocked' && (rstate === 'starting' || !!ft8.toggleBusy);
        const caption =
          rstate === 'listening'
            ? [ft8.band ?? null, ft8.decodesPerMin != null ? `${ft8.decodesPerMin.toFixed(1)}/min` : null]
                .filter((part): part is string => !!part)
                .join(' · ')
            : '';
        const label =
          rstate === 'off'
            ? 'Off'
            : rstate === 'starting'
              ? 'Starting…'
              : rstate === 'listening'
                ? 'Listening'
                : (blockedLabel ?? 'Blocked');
        const title =
          rstate === 'off'
            ? 'Start FT-8 listening'
            : rstate === 'starting'
              ? 'Starting FT-8…'
              : rstate === 'listening'
                ? 'FT-8 listening — click to stop'
                : `FT-8 blocked (${blockedLabel ?? 'needs attention'}) — click to open the panel`;
        const dotClass =
          rstate === 'listening'
            ? ''
            : rstate === 'starting'
              ? 'connecting'
              : rstate === 'blocked'
                ? 'blocked'
                : 'idle';
        return (
          <>
            <div className="dash-divider" />
            <div className="dash-item dash-ft8">
              <div className="dash-label">FT-8</div>
              <button
                type="button"
                className="dash-ft8-control"
                data-testid="dash-ft8-control"
                data-state={rstate}
                aria-pressed={rstate === 'listening'}
                disabled={busy}
                onClick={rstate === 'blocked' ? ft8.onOpen : (ft8.onToggleListening ?? ft8.onOpen)}
                title={title}
              >
                <span className={`dash-status-dot ${dotClass}`} aria-hidden="true" />
                <span className="dash-ft8-state">{label}</span>
                {caption && (
                  <span className="dash-ft8-caption" data-testid="dash-ft8-caption">
                    {caption}
                  </span>
                )}
              </button>
            </div>
          </>
        );
      })()}

      {/* Merged Elmer × Agent-send control — ONE ribbon slot. Display-only:
          shows arm/taint state at a glance and opens the Elmer drawer on click.
          Arm/disarm/re-arm were relocated to the drawer header (see ElmerPane),
          which frees the slot that was pushing Connect off-screen at 1080p. */}
      {onOpenElmer && (
        <>
          <div className="dash-divider" />
          <ElmerAgentChip
            status={egress?.status ?? EGRESS_STATUS_DISARMED}
            elmerOpen={elmerOpen}
            onOpenElmer={onOpenElmer}
          />
        </>
      )}

      {onConnect && (
        <>
          <div className="dash-divider" />
          <div className="dash-item dash-connect">
            <Button
              tone="primary"
              emphasis="solid"
              size="sm"
              className="connect-button"
              onClick={onConnect}
              disabled={connecting}
              data-testid="connect-button"
            >
              {connecting ? 'Connecting…' : 'Connect'}
            </Button>
            {connecting && onAbort && (
              <Button
                tone="danger"
                emphasis="outline"
                size="sm"
                className="abort-button"
                onClick={onAbort}
                data-testid="abort-button"
              >
                Abort
              </Button>
            )}
          </div>
        </>
      )}
    </div>
  );
});
