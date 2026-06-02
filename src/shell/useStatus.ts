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

import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
  /** CMS server host the operator dials (tuxlink-3o0). The inline SettingsPanel
   * loads this into its host text input on open. */
  host: string;
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

/** Mirrors PositionStatusDto from ui_commands.rs (tuxlink-va1i: amended for the
 * UI/on-air locator decoupling — see spec 2026-06-01-position-subsystem-
 * restoration-design.md §2.5 + §4.1).
 *
 * Live arbiter state — NOT config. Polled at 2s by useStatusData.
 *
 * Two-helper split (tuxlink-va1i): the backend now exposes BOTH
 *   - `broadcast_grid` — effective ON-AIR locator (what would be transmitted)
 *   - `ui_grid` — effective LOCAL DISPLAY locator (what the ribbon shows)
 * They coincide in most states; under LocalUiOnly + source=Gps + fresh fix they
 * intentionally diverge: ui_grid reflects the live precision-reduced fix while
 * broadcast_grid stays at the static config grid (privacy honored, local
 * visibility intact). Empty string means no grid is available.
 *
 * Per spec §4.1: the source chip reads `position_source` from `config_read` —
 * NOT from this DTO. Sticky-Manual is preserved at the config boundary;
 * live-status is grid-availability only. */
export interface PositionStatusDto {
  gps_ready: boolean;
  /** Effective ON-AIR locator (what would be transmitted, honoring gps_state +
   *  precision). Empty = no grid. Distinct from `ui_grid` under LocalUiOnly. */
  broadcast_grid: string;
  /** Effective LOCAL DISPLAY locator for the ribbon (tuxlink-va1i, spec §2.5 +
   *  §4.1). Empty = no grid. Distinct from `broadcast_grid` under LocalUiOnly +
   *  source=Gps + fresh fix: ui_grid shows the live fix; broadcast_grid stays
   *  at the static config grid. */
  ui_grid: string;
}

/**
 * Mirrors BackendStatus from winlink_backend.rs.
 * Uses a discriminated union on `kind` (matching the Rust serde tag).
 */
export type StatusDto =
  | { kind: 'Disconnected' }
  | { kind: 'Connecting'; transport: string }
  // Packet armed-but-idle (listening to answer an inbound call). Distinct from
  // Connecting (an active dial). Renders "Listening · Packet 1200". (tuxlink-orj)
  | { kind: 'Listening'; transport: string }
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
    case 'Listening':
      // Packet armed-but-idle — honest "Listening", not the prior "Connecting" lie.
      return `Listening · ${normalizeTransportLabel(status.transport)}`;
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
 * `null` (no backend — the default when nothing is configured) and Disconnected both read "Idle".
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
    case 'Listening':
      // Armed + ready to answer → healthy state (green dot, spec §4.6).
      return { label: 'Listening', tone: 'good' };
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
  /**
   * Raw live backend status (or null when no backend / pre-wizard). Exposed so
   * transport-specific indicators (e.g. the packet ribbon item, tuxlink-orj) can
   * derive their own state from the same poll the CMS labels use, rather than
   * re-polling. The CMS-facing fields above are pre-derived from this.
   */
  status?: StatusDto | null;
}

/**
 * Query keys for the status hook's three polls. Exported so DashboardRibbon
 * (and any future write-path) can target the cache with `invalidateQueries`
 * and trigger an immediate refetch (tuxlink-i9vn — pre-refactor T14 invalidated
 * `['config_read']` against a raw setInterval that ignored it).
 */
export const STATUS_QUERY_KEYS = {
  config: ['config_read'] as const,
  backend: ['backend_status'] as const,
  position: ['position_status'] as const,
};

/**
 * Poll config_read (5s) + backend_status (2s) + position_status (2s) via
 * react-query, and derive the StatusBar's display values via the pure
 * formatters above. The hook surfaces callsign/grid/state in the status bar.
 *
 * The previous implementation used raw useState + useEffect + setInterval. T14
 * (DashboardRibbon write paths) added `queryClient.invalidateQueries({
 * queryKey: ['config_read'] })` after grid/source edits to flip the source
 * chip within one render cycle — but that invalidate had no real refetch
 * target. tuxlink-i9vn converts the polls to useQuery so invalidation actually
 * triggers a refetch.
 *
 * All three commands degrade gracefully: config absent → empty callsign/grid;
 * backend None → status null → "Idle"; gpsd unavailable → gpsReady false.
 * `enabled: !DEV_FIXTURE` keeps the dev fixture path free of invocations.
 */
export function useStatusData(): StatusBarData {
  const queryClient = useQueryClient();

  const configQuery = useQuery({
    queryKey: STATUS_QUERY_KEYS.config,
    queryFn: () => invoke<ConfigViewDto>('config_read'),
    refetchInterval: 5000,
    enabled: !DEV_FIXTURE,
    // App.tsx already sets retry:false globally; restate here so this hook's
    // semantics survive any future change to the root QueryClient defaults.
    retry: false,
  });

  const backendQuery = useQuery({
    queryKey: STATUS_QUERY_KEYS.backend,
    queryFn: () => invoke<StatusDto | null>('backend_status'),
    refetchInterval: 2000,
    enabled: !DEV_FIXTURE,
    retry: false,
  });

  const positionQuery = useQuery({
    queryKey: STATUS_QUERY_KEYS.position,
    queryFn: () => invoke<PositionStatusDto>('position_status'),
    refetchInterval: 2000,
    enabled: !DEV_FIXTURE,
    retry: false,
  });

  // Event-driven path (2026-05-31): backend emits `backend_status:change` on
  // every transition (see src-tauri/src/bootstrap.rs). Without this, the 2s
  // poll missed sub-second CMS-Z exchanges and the user only saw Connecting
  // → Disconnected without the brief Connected window. We poke the
  // react-query cache directly via setQueryData so the listener and the
  // refetchInterval write to the same place. The 2s poll stays as a snapshot
  // backstop in case events drop (broadcast-channel overflow, late-mounting
  // UI, etc.).
  useEffect(() => {
    if (DEV_FIXTURE) return;
    let mounted = true;
    let unlisten: (() => void) | null = null;
    listen<StatusDto>('backend_status:change', (event) => {
      if (mounted) queryClient.setQueryData(STATUS_QUERY_KEYS.backend, event.payload);
    })
      .then((u) => {
        if (mounted) unlisten = u;
        else u();
      })
      .catch(() => {
        // listen() unavailable (test env / no Tauri context) — the poll alone
        // still works.
      });
    return () => {
      mounted = false;
      if (unlisten) unlisten();
    };
  }, [queryClient]);

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
      status: null,
    };
  }

  // useQuery returns `undefined` until the first success; the pre-refactor
  // code used `null` for the unloaded state. Normalize to the prior null
  // semantics so downstream branching (`if (config)`) keeps working.
  const config: ConfigViewDto | null = configQuery.data ?? null;
  // backend_status's queryFn can return null (Rust `Option<BackendStatus>`).
  // useQuery's data may also be undefined pre-load. Both map to null.
  const status: StatusDto | null = backendQuery.data ?? null;
  // position_status: keep the "last known value" semantics. useQuery already
  // does this — on a transient rejection, data stays at the previous success.
  // Pre-load is undefined → null.
  const positionStatus: PositionStatusDto | null = positionQuery.data ?? null;

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

  // tuxlink-va1i (spec §2.5 + §4.1): source the ribbon's grid from the LIVE
  // position_status.ui_grid — the effective LOCAL DISPLAY locator. Distinct
  // from broadcast_grid: under LocalUiOnly + source=Gps + fresh fix, ui_grid
  // reflects the live precision-reduced fix (operator sees their actual
  // location) while broadcast_grid stays at the static config grid (privacy
  // honored on-air). Pre-va1i the derivation read broadcast_grid and the two
  // concerns were collapsed onto one helper; the amendment restores the
  // distinction. Falls back to the config-derived grid when position_status
  // has not yet loaded or returns an empty string (pre-wizard, gpsd
  // unavailable, etc.).
  const liveGrid = positionStatus?.ui_grid
    ? positionStatus.ui_grid
    : null;
  const ribbonGrid = liveGrid ?? gridResult.broadcast;

  return {
    callsign,
    grid: ribbonGrid,
    gridTooltip: gridResult.tooltip,
    state: formatStatusState(status),
    connection: formatConnectionState(status, configTransport),
    // Per spec §4.1 (position-subsystem-restoration, tuxlink-c79g): source chip
    // reads from the stored config preference, NOT from live position_status.
    // This preserves sticky-Manual at the frontend boundary — a fresh GPS fix
    // does not flip the chip back to Gps. Defaults to 'Gps' (project default-on)
    // until config_read resolves.
    position_source: config?.position_source ?? 'Gps',
    gpsReady: positionStatus?.gps_ready ?? false,
    status,
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
  // Packet transport ("Packet-7" etc.) → fixed 1200-baud label (spec §4.6).
  if (transport.startsWith('Packet')) return 'Packet 1200';
  // Unknown transport string: pass through as-is
  return transport;
}
