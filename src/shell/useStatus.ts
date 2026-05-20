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
 * - backend_status + config_read commands are NOT registered in lib.rs yet — that lands
 *   in the orchestrator integration commit (spec §4.3). Components are tested against
 *   synthetic DTO values here.
 */

// ============================================================================
// DTOs — mirror the Rust command serialization shapes (spec §3.2)
// ============================================================================

export type CmsTransport = 'CmsSsl' | 'Telnet';

export type GpsState = 'Off' | 'LocalUiOnly' | 'BroadcastAtPrecision';

export type PositionPrecision = 'FourCharGrid' | 'SixCharGrid';

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
      return `Error: ${status.reason}`;
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
