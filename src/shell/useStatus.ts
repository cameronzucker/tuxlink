/**
 * useStatus — pure formatters + config/status types for Task 16 ribbon + status bar.
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.6
 * bd issue: tuxlink-hvv
 *
 * Design notes:
 * - All exported formatter functions are pure (no I/O, no side effects) — the prime unit-test targets.
 * - ConfigViewDto / StatusDto mirror the Rust commands' serialization shapes (spec §3.2).
 * - The `useStatus` React hook (bottom of this file) composes these into a single
 *   query the ribbon consumes. It mocks `invoke` in tests via vitest.mock.
 * - backend_status + config_read commands ARE registered in lib.rs (orchestrator
 *   integration commit, spec §4.3). Pure formatters are tested against synthetic
 *   DTO values; the `useStatusData` hook (bottom) is tested via mocked invoke.
 */

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { DEV_FIXTURE, DEV_CALLSIGN, DEV_GRID } from '../mailbox/devFixture';

// ============================================================================
// DTOs — mirror the Rust command serialization shapes (spec §3.2)
// ============================================================================

export type CmsTransport = 'CmsSsl' | 'Telnet';

export type GpsState = 'Off' | 'LocalUiOnly' | 'BroadcastAtPrecision';

export type PositionPrecision = 'FourCharGrid' | 'SixCharGrid';

/** Mirrors PositionSource from config.rs (tuxlink-686). Default `Gps`. */
export type PositionSource = 'Manual' | 'Gps';

/** Mirrors the Rust ConfigViewDto returned by the config_read command. */
export interface ConfigViewDto {
  connect_to_cms: boolean;
  /** CmsTransport enum value from config.rs */
  transport: CmsTransport;
  /** Present when connect_to_cms=true; null for offline installs */
  callsign: string | null;
  /** Free-form station identifier for offline-mode operators */
  identifier: string | null;
  /** Maidenhead grid, stored at full 6-char precision; null if not set */
  grid: string | null;
  gps_state: GpsState;
  position_precision: PositionPrecision;
  /** Active position source (tuxlink-686): `Gps` (default) or `Manual` when
   * the operator has pinned a grid square. Task 8 renders a source chip. */
  position_source: PositionSource;
}

/** Mirrors PositionStatusDto from ui_commands.rs (tuxlink-686, Task 11 + Codex P1-B).
 * Live arbiter state — NOT config. Polled at 2s by useStatusData.
 * `broadcast_grid` is the effective on-air locator (honoring gps_state) — the
 * ribbon shows this so it always matches what is/would be transmitted. Empty
 * string means no grid is available. */
export interface PositionStatusDto {
  gps_ready: boolean;
  /** Effective on-air locator (honoring gps_state + precision). Empty = no grid. */
  broadcast_grid: string;
}

/**
 * Mirrors BackendStatus from winlink_backend.rs.
 * Uses a discriminated union on `kind` (matching the Rust serde tag).
 */
export type StatusDto =
  | { kind: 'Disconnected' }
  | { kind: 'Connecting'; transport: string }
  | { kind: 'Connected'; transport: string; peer: string; since_iso: string }
  | { kind: 'Disconnecting' }
  | { kind: 'Error'; reason: string };

// ============================================================================
// Pure formatter functions — unit-tested in status.test.ts
// ============================================================================

/**
 * Map a raw backend error reason to a CONCISE, human-readable ribbon label.
 *
 * The ribbon is a status strip, not an error console — dumping the raw reason
 * (`Error: <long telnet/CMS reason>`) reflowed the layout and read as machine
 * noise (ng3 re-smoke #5). The FULL reason still goes to the session log; this is
 * the at-a-glance status. Kept short enough to fit the ribbon without truncation.
 */
export function humanizeConnectionError(reason: string): string {
  const r = reason.toLowerCase();
  if (/not registered|unregistered|unknown client|\bsid\b/.test(r)) return 'Rejected — not registered';
  if (/timed out|timeout/.test(r)) return 'Connection timed out';
  if (/refused|unreachable|no route|dns|resolve|connect/.test(r)) return 'CMS unreachable';
  if (/password|secure login|auth|login|credential/.test(r)) return 'Login failed';
  if (/\btls\b|\bssl\b|certificate|handshake/.test(r)) return 'Secure-connection error';
  // Fallback: a trimmed first clause, else a generic label (never the full dump).
  const first = reason.split(/[\n.;]/)[0].trim();
  return first.length > 0 && first.length <= 40 ? first : 'Connection failed';
}

/**
 * Format the connection state label for the ribbon.
 *
 * When the backend is online (`status != null`), renders the live BackendStatus.
 * When offline / pre-connect (`status == null`), falls back to a config-derived
 * "Idle · <transport>" label using the configured CmsTransport.
 *
 * Per spec §5.6 (Codex verdict V6): the ribbon consumes live status() when the
 * backend exists; falls back to config-derived stub otherwise.
 */
export function formatConnectionState(
  status: StatusDto | null,
  configTransport: CmsTransport,
): string {
  if (status === null) {
    return `Idle · ${formatTransportLabel(configTransport)}`;
  }

  switch (status.kind) {
    case 'Disconnected':
      // Spec §5.6: connection state ALWAYS names the transport. The
      // Disconnected variant carries no transport string of its own, so fall
      // back to the configured transport (same source as the Idle label).
      return `Disconnected · ${formatTransportLabel(configTransport)}`;
    case 'Connecting':
      return `Connecting · ${normalizeTransportLabel(status.transport)}`;
    case 'Connected': {
      const label = normalizeTransportLabel(status.transport);
      return `Connected · ${label}`;
    }
    case 'Disconnecting':
      return 'Disconnecting';
    case 'Error':
      // Concise human-readable label (ng3 re-smoke #5); full reason → session log.
      return humanizeConnectionError(status.reason);
  }
}

/**
 * Format the callsign to display in the ribbon.
 *
 * Uses identity.callsign for CMS-connected installs; falls back to
 * identity.identifier for offline installs (spec §5.6).
 */
export function formatCallsign(opts: {
  connect_to_cms: boolean;
  callsign: string | null;
  identifier: string | null;
}): string {
  // Prefer callsign regardless of connect_to_cms (handles edge cases where
  // both are set — spec says callsign takes priority).
  if (opts.callsign) return opts.callsign;
  if (opts.identifier) return opts.identifier;
  return '';
}

/**
 * Format the grid locator for ribbon display.
 *
 * Returns the 4-char broadcast grid; the 6-char tooltip is only populated
 * when position_precision == 'SixCharGrid' AND the stored grid is > 4 chars.
 *
 * Per spec §5.6 + Principle 7: 4-char is the default broadcast; 6-char is
 * opt-in. The stored grid is always at full precision; we truncate for broadcast.
 */
export function formatGrid(opts: {
  grid: string | null;
  precision: PositionPrecision;
}): { broadcast: string | null; tooltip: string | null } {
  if (!opts.grid) {
    return { broadcast: null, tooltip: null };
  }

  const broadcast = opts.grid.substring(0, 4) || null;

  // Show 6-char tooltip only when: precision is SixCharGrid AND stored grid
  // has more than 4 chars (i.e. we actually have the 6-char form).
  const tooltip =
    opts.precision === 'SixCharGrid' && opts.grid.length > 4
      ? opts.grid.substring(0, 6)
      : null;

  return { broadcast, tooltip };
}

/**
 * Map a GpsState enum value to a human-readable ribbon label.
 *
 * Per spec §5.6: GPS status on/manual/off/searching maps each gps_state.
 * The displayed values correspond to the spec's GPS state variants.
 */
export function formatGpsStatus(gpsState: GpsState): string {
  switch (gpsState) {
    case 'Off':
      return 'GPS off';
    case 'LocalUiOnly':
      return 'GPS local UI only';
    case 'BroadcastAtPrecision':
      return 'GPS on';
  }
}

// ============================================================================
// Status-bar state (Mock D — tuxlink-yd4)
// ============================================================================

export type StatusTone = 'idle' | 'good' | 'warn' | 'error';

/**
 * Map BackendStatus to the Mock D status-bar's short state word + dot tone.
 *
 * Mock D's status bar shows a single state word (`Idle`, `Connecting`, …) with
 * a colored dot, NOT the ribbon's "Idle · Telnet" transport-qualified label.
 * `null` (no backend — the v0.0.1 default) and Disconnected both read "Idle".
 */
export function formatStatusState(status: StatusDto | null): { label: string; tone: StatusTone } {
  if (status === null) return { label: 'Idle', tone: 'idle' };
  switch (status.kind) {
    case 'Disconnected':
      return { label: 'Idle', tone: 'idle' };
    case 'Connecting':
      return { label: 'Connecting', tone: 'warn' };
    case 'Connected':
      return { label: 'Connected', tone: 'good' };
    case 'Disconnecting':
      return { label: 'Disconnecting', tone: 'warn' };
    case 'Error':
      return { label: 'Error', tone: 'error' };
  }
}

// ============================================================================
// useStatusData — the StatusBar's live data hook (Mock D)
// ============================================================================

export interface StatusBarData {
  /** Callsign (or offline identifier); '' until config loads. */
  callsign: string;
  /** 4-char broadcast grid; null when unset. */
  grid: string | null;
  /** 6-char grid for the tooltip when precision is opted up; else null. */
  gridTooltip: string | null;
  /** Short state word + dot tone derived from BackendStatus. */
  state: { label: string; tone: StatusTone };
  /**
   * Full ribbon connection string — e.g. "Idle · CMS-SSL", "Connected · CMS-SSL",
   * "Error: <reason>". Derived from formatConnectionState(status, configTransport)
   * so the transport label always reflects the real configured/active transport
   * rather than a hardcoded suffix.
   */
  connection: string;
  /** Active position source (tuxlink-686). Task 8 renders a source chip from this. */
  position_source: PositionSource;
  /**
   * Whether a usable GPS fix is currently available (tuxlink-686, Task 11).
   * Optional — Task 11 populates it; until then it is `undefined` which the
   * GridEdit consumer treats as `false` (GPS-ready affordance stays hidden).
   */
  gpsReady?: boolean;
}

/**
 * Poll config_read (5s) + backend_status (2s) and derive the StatusBar's
 * display values via the pure formatters above. This is the status fetch that
 * lived in DashboardRibbon (now parked); Mock D surfaces callsign/grid/state in
 * the status bar instead of a top ribbon. Both commands degrade gracefully:
 * config absent → empty callsign/grid; backend None → status null → "Idle".
 */
export function useStatusData(): StatusBarData {
  const [config, setConfig] = useState<ConfigViewDto | null>(null);
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [positionStatus, setPositionStatus] = useState<PositionStatusDto | null>(null);

  useEffect(() => {
    if (DEV_FIXTURE) return; // dev fixture supplies fixed config; don't poll
    let mounted = true;
    const load = () => {
      invoke<ConfigViewDto>('config_read')
        .then((c) => {
          if (mounted) setConfig(c);
        })
        .catch(() => {
          /* config absent / pre-wizard: status bar shows just the state word */
        });
    };
    load();
    const id = setInterval(load, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, []);

  useEffect(() => {
    if (DEV_FIXTURE) return; // dev fixture is always "Idle"; don't poll
    let mounted = true;
    const load = () => {
      invoke<StatusDto | null>('backend_status')
        .then((s) => {
          if (mounted) setStatus(s ?? null);
        })
        .catch(() => {
          if (mounted) setStatus(null);
        });
    };
    load();
    const id = setInterval(load, 2000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, []);

  // tuxlink-686 Task 11: poll position_status (live arbiter, NOT config) at 2s.
  // Populates gpsReady for the ribbon's "GPS ready — tap to switch" affordance.
  // Degrades gracefully on error (catch → leave null → gpsReady: false).
  useEffect(() => {
    if (DEV_FIXTURE) return;
    let mounted = true;
    const load = () => {
      invoke<PositionStatusDto>('position_status')
        .then((ps) => {
          if (mounted) setPositionStatus(ps);
        })
        .catch(() => {
          // gpsd error/blip: keep the last known value (don't clear — avoids flashing the affordance off on a single missed poll)
        });
    };
    load();
    const id = setInterval(load, 2000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, []);

  // Dev fixture: report the mock's fixed station (W4PHS · EM75xx · Idle) so the
  // status bar + window title reproduce the mock instead of the live config.
  if (DEV_FIXTURE) {
    return {
      callsign: DEV_CALLSIGN,
      grid: DEV_GRID,
      gridTooltip: null,
      state: { label: 'Idle', tone: 'idle' },
      connection: 'Idle · CMS-SSL',
      position_source: 'Gps',
      gpsReady: false,
    };
  }

  const callsign = config
    ? formatCallsign({
        connect_to_cms: config.connect_to_cms,
        callsign: config.callsign,
        identifier: config.identifier,
      })
    : '';

  const gridResult = config
    ? formatGrid({ grid: config.grid, precision: config.position_precision })
    : { broadcast: null, tooltip: null };

  // Use the configured transport when building the connection string.  When
  // config hasn't loaded yet, fall back to 'CmsSsl' so the label is
  // informative (it will be correct once the first poll completes).
  const configTransport: CmsTransport = config?.transport ?? 'CmsSsl';

  // Codex P1-B: source the ribbon's grid from the LIVE position_status
  // broadcast_grid (the effective on-air locator, honoring gps_state). Falls
  // back to the config-derived grid when position_status has not yet loaded or
  // returns an empty string (pre-wizard, gpsd unavailable, etc.). This ensures
  // the ribbon always shows exactly what is/would be transmitted.
  const liveGrid = positionStatus?.broadcast_grid
    ? positionStatus.broadcast_grid
    : null;
  const ribbonGrid = liveGrid ?? gridResult.broadcast;

  return {
    callsign,
    grid: ribbonGrid,
    gridTooltip: gridResult.tooltip,
    state: formatStatusState(status),
    connection: formatConnectionState(status, configTransport),
    position_source: config?.position_source ?? 'Gps',
    gpsReady: positionStatus?.gps_ready ?? false,
  };
}

// ============================================================================
// Internal helpers
// ============================================================================

/** Map CmsTransport enum value to user-facing label. */
function formatTransportLabel(transport: CmsTransport): string {
  switch (transport) {
    case 'CmsSsl':
      return 'CMS-SSL';
    case 'Telnet':
      return 'Telnet';
  }
}

/**
 * Normalize a transport string from BackendStatus (which may be "CMS-CmsSsl",
 * "CMS-Telnet", etc.) to a user-facing label.
 *
 * PatBackend.connect() produces `format!("CMS-{:?}", mode)` which yields
 * "CMS-CmsSsl" or "CMS-Telnet". We normalize these for display.
 */
function normalizeTransportLabel(transport: string): string {
  if (transport.includes('CmsSsl') || transport.includes('Ssl')) return 'CMS-SSL';
  if (transport.includes('Telnet')) return 'Telnet';
  // Unknown transport string: pass through as-is
  return transport;
}
