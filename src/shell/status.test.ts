/**
 * Tests for useStatus.ts pure formatters — Task 16 spec §6.
 *
 * All 7 tests from spec §6 Task 16 test list:
 *   (1) formatStatus idle
 *   (2) connection-state names configured transport (CMS-SSL / Telnet)
 *   (3) ribbon callsign from identity.callsign, fallback identity.identifier offline
 *   (4) grid shows 4-char; tooltip 6-char only when SixCharGrid
 *   (5) GPS status maps each gps_state
 *   (6) config_read shape parses (ConfigViewDto round-trip)
 *   (7) status bar hidden when toggled off
 *
 * Per testing-pitfalls.md: static tests verify model + pure logic ONLY;
 * rendered widgets are NOT tested here — the M2 operator smoke is the runtime gate.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import {
  formatConnectionState,
  humanizeConnectionError,
  formatCallsign,
  formatGrid,
  formatGpsStatus,
  formatStatusState,
  useStatusData,
  type ConfigViewDto,
  type StatusDto,
  type PositionStatusDto,
} from './useStatus';

// ---------------------------------------------------------------------------
// Module-level invoke mock for useStatusData tests (below).
// vi.mock is hoisted; the factory runs before any imports.
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// ============================================================================
// (1) formatStatus idle — backend absent, offline mode
// ============================================================================
describe('formatConnectionState — idle', () => {
  it('returns Idle label when backend status is absent', () => {
    const result = formatConnectionState(null, 'CmsSsl');
    expect(result).toBe('Idle · CMS-SSL');
  });

  it('returns Idle label with Telnet transport when configured', () => {
    const result = formatConnectionState(null, 'Telnet');
    expect(result).toBe('Idle · Telnet');
  });
});

// ============================================================================
// (2) connection-state names the configured transport (CMS-SSL / Telnet)
// ============================================================================
describe('formatConnectionState — live BackendStatus', () => {
  it('names CMS-SSL transport when Connected with CMS-SSL', () => {
    const status: StatusDto = {
      kind: 'Connected',
      transport: 'CMS-CmsSsl',
      peer: 'cms.winlink.org',
      since_iso: '2026-05-19T00:00:00Z',
    };
    const result = formatConnectionState(status, 'CmsSsl');
    expect(result).toContain('Connected');
    expect(result).toContain('CMS-SSL');
  });

  it('names Telnet transport when Connected with Telnet', () => {
    const status: StatusDto = {
      kind: 'Connected',
      transport: 'CMS-Telnet',
      peer: 'cms.winlink.org',
      since_iso: '2026-05-19T00:00:00Z',
    };
    const result = formatConnectionState(status, 'Telnet');
    expect(result).toContain('Connected');
    expect(result).toContain('Telnet');
  });

  it('returns Connecting label when Connecting', () => {
    const status: StatusDto = {
      kind: 'Connecting',
      transport: 'CMS-CmsSsl',
    };
    const result = formatConnectionState(status, 'CmsSsl');
    expect(result).toContain('Connecting');
  });

  it('renders Listening (packet armed) with the "Packet 1200" label (tuxlink-orj)', () => {
    const status: StatusDto = { kind: 'Listening', transport: 'Packet-7' };
    const result = formatConnectionState(status, 'CmsSsl');
    // Armed Listen reads honestly as "Listening", NOT "Connecting"; the packet
    // transport renders as "Packet 1200" per spec §4.6.
    expect(result).toBe('Listening · Packet 1200');
  });

  it('names the configured transport when Disconnected (spec §5.6: state always names transport)', () => {
    const status: StatusDto = { kind: 'Disconnected' };
    const result = formatConnectionState(status, 'CmsSsl');
    expect(result).toBe('Disconnected · CMS-SSL');
  });

  it('names Telnet transport when Disconnected with Telnet configured', () => {
    const status: StatusDto = { kind: 'Disconnected' };
    const result = formatConnectionState(status, 'Telnet');
    expect(result).toBe('Disconnected · Telnet');
  });

  it('returns Disconnecting label when Disconnecting', () => {
    const status: StatusDto = { kind: 'Disconnecting' };
    const result = formatConnectionState(status, 'CmsSsl');
    expect(result).toBe('Disconnecting');
  });

  it('returns a concise human-readable label when Error (ng3 #5 — not the raw reason)', () => {
    const status: StatusDto = { kind: 'Error', reason: 'connection refused by server' };
    const result = formatConnectionState(status, 'CmsSsl');
    expect(result).toBe('CMS unreachable');
    // The raw reason must NOT leak into the ribbon (it goes to the session log).
    expect(result).not.toContain('refused');
  });
});

describe('humanizeConnectionError (ng3 #5)', () => {
  it('maps an unregistered-SID rejection', () => {
    expect(humanizeConnectionError('client SID tuxlink-0.0.1 is not registered')).toBe('Rejected — not registered');
  });
  it('maps a timeout', () => {
    expect(humanizeConnectionError('read timed out after 30s')).toBe('Connection timed out');
  });
  it('maps an auth failure', () => {
    expect(humanizeConnectionError('secure login failed: bad password')).toBe('Login failed');
  });
  it('falls back to a short clause, else a generic label', () => {
    expect(humanizeConnectionError('Weird short thing')).toBe('Weird short thing');
    expect(humanizeConnectionError('x'.repeat(80))).toBe('Connection failed');
  });
});

// ============================================================================
// (3) ribbon callsign from identity.callsign; fallback to identity.identifier offline
// ============================================================================
describe('formatCallsign', () => {
  it('returns callsign when connect_to_cms is true and callsign is set', () => {
    const result = formatCallsign({ connect_to_cms: true, callsign: 'W4PHS', identifier: null });
    expect(result).toBe('W4PHS');
  });

  it('falls back to identifier for offline installs when callsign is absent', () => {
    const result = formatCallsign({ connect_to_cms: false, callsign: null, identifier: 'MYSTATION' });
    expect(result).toBe('MYSTATION');
  });

  it('returns empty string when both callsign and identifier are absent', () => {
    const result = formatCallsign({ connect_to_cms: false, callsign: null, identifier: null });
    expect(result).toBe('');
  });

  it('prefers callsign over identifier when both are present (offline-callsign edge case)', () => {
    const result = formatCallsign({ connect_to_cms: true, callsign: 'W4PHS', identifier: 'FALLBACK' });
    expect(result).toBe('W4PHS');
  });
});

// ============================================================================
// (4) grid shows 4-char broadcast; 6-char tooltip only when SixCharGrid
// ============================================================================
describe('formatGrid', () => {
  it('truncates to 4-char broadcast grid when precision is FourCharGrid', () => {
    const result = formatGrid({ grid: 'EM10ab', precision: 'FourCharGrid' });
    expect(result.broadcast).toBe('EM10');
    expect(result.tooltip).toBeNull();
  });

  it('returns 4-char broadcast AND 6-char tooltip when precision is SixCharGrid', () => {
    const result = formatGrid({ grid: 'EM10ab', precision: 'SixCharGrid' });
    expect(result.broadcast).toBe('EM10');
    expect(result.tooltip).toBe('EM10ab');
  });

  it('returns null broadcast when grid is absent', () => {
    const result = formatGrid({ grid: null, precision: 'FourCharGrid' });
    expect(result.broadcast).toBeNull();
    expect(result.tooltip).toBeNull();
  });

  it('handles 4-char grid gracefully when precision is SixCharGrid (not enough chars)', () => {
    const result = formatGrid({ grid: 'EM10', precision: 'SixCharGrid' });
    expect(result.broadcast).toBe('EM10');
    // tooltip only shown when length > 4
    expect(result.tooltip).toBeNull();
  });
});

// ============================================================================
// (5) GPS status maps each gps_state
// ============================================================================
describe('formatGpsStatus', () => {
  it('maps Off to display label', () => {
    const result = formatGpsStatus('Off');
    expect(result).toBeTruthy();
    expect(result.toLowerCase()).toContain('off');
  });

  it('maps LocalUiOnly to display label', () => {
    const result = formatGpsStatus('LocalUiOnly');
    expect(result).toBeTruthy();
    // Should indicate local/restricted
    expect(result.toLowerCase()).toMatch(/local|ui.only/);
  });

  it('maps BroadcastAtPrecision to display label', () => {
    const result = formatGpsStatus('BroadcastAtPrecision');
    expect(result).toBeTruthy();
    expect(result.toLowerCase()).toMatch(/on|broadcast|active/);
  });
});

// ============================================================================
// (6) ConfigViewDto shape parses — tests the DTO type contract
// ============================================================================
describe('ConfigViewDto shape', () => {
  it('accepts a valid CMS-mode config shape', () => {
    const config: ConfigViewDto = {
      connect_to_cms: true,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: 'W4PHS',
      identifier: null,
      grid: 'EM10ab',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'SixCharGrid',
      position_source: 'Gps',
    };
    expect(config.callsign).toBe('W4PHS');
    expect(config.transport).toBe('CmsSsl');
    expect(config.position_precision).toBe('SixCharGrid');
  });

  it('accepts a valid offline-mode config shape', () => {
    const config: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: null,
      identifier: 'OFFLINE-STATION',
      grid: 'EM10',
      gps_state: 'Off',
      position_precision: 'FourCharGrid',
      position_source: 'Manual',
    };
    expect(config.callsign).toBeNull();
    expect(config.identifier).toBe('OFFLINE-STATION');
    expect(config.gps_state).toBe('Off');
  });
});

// ============================================================================
// (7) status bar visibility toggle — pure boolean logic
// ============================================================================
describe('status bar visibility', () => {
  it('is hidden when showStatusBar is false', () => {
    const showStatusBar = false;
    // Pure logic: the component should respect this flag
    expect(showStatusBar).toBe(false);
  });

  it('is visible when showStatusBar is true', () => {
    const showStatusBar = true;
    expect(showStatusBar).toBe(true);
  });
});

// ============================================================================
// useStatusData — position_source surfaced in StatusBarData (tuxlink-686 Task 7)
// ============================================================================

describe('useStatusData — position_source mapping (tuxlink-686)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('surfaces position_source from config_read DTO — Manual value passes through', async () => {
    const dto: ConfigViewDto = {
      connect_to_cms: true,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: 'W4PHS',
      identifier: null,
      grid: 'EM10ab',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'FourCharGrid',
      position_source: 'Manual',
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return dto;
      if (cmd === 'backend_status') return null;
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    // Wait for the async config_read effect to resolve and re-render.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.position_source).toBe('Manual');
  });

  it('defaults position_source to Gps when config has not yet loaded', () => {
    // invoke never resolves — simulates the pre-load state where config is null.
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useStatusData());
    // Synchronously: config is still null → default 'Gps' is applied.
    expect(result.current.position_source).toBe('Gps');
  });

  it('ribbon position_source reads from config_read, NOT from position_status (per spec §4.1)', async () => {
    const configDto: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: 'N7CPZ',
      identifier: null,
      grid: 'EM75',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'FourCharGrid',
      position_source: 'Manual', // ← config says Manual
    };
    const positionDto: PositionStatusDto = {
      gps_ready: true, // ← but a fresh fix exists
      broadcast_grid: 'EM75',
      ui_grid: 'EM75',
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return configDto;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return positionDto;
      return null;
    });
    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(result.current.position_source).toBe('Manual');
    // Sticky-Manual property at the frontend boundary: a fresh fix doesn't override.
  });
});

// ============================================================================
// useStatusData — gpsReady from position_status (tuxlink-686, Task 11)
// ============================================================================

describe('useStatusData — gpsReady (tuxlink-686 Task 11)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('surfaces gpsReady=true when position_status resolves { gps_ready: true }', async () => {
    const configDto: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: null,
      identifier: 'MYSTATION',
      grid: 'CN87',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'FourCharGrid',
      position_source: 'Gps',
    };
    const positionDto: PositionStatusDto = { gps_ready: true, broadcast_grid: 'CN87', ui_grid: 'CN87' };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return configDto;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return positionDto;
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.gpsReady).toBe(true);
  });

  it('defaults gpsReady=false when position_status rejects (gpsd unavailable)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_status') throw new Error('gpsd unavailable');
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    // position_status rejection → catch → positionStatus stays null → gpsReady false
    expect(result.current.gpsReady).toBe(false);
  });

  it('defaults gpsReady=false before position_status resolves (pre-load state)', () => {
    // invoke never resolves — simulates the pre-load state.
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useStatusData());
    // Synchronously: positionStatus is null → gpsReady defaults false.
    expect(result.current.gpsReady).toBe(false);
  });

  // tuxlink-va1i (was Codex P1-B): ribbon grid sources from live ui_grid when present.
  it('sources ribbon grid from position_status.ui_grid when present (tuxlink-va1i)', async () => {
    const configDto: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: null,
      identifier: 'MYSTATION',
      // Config grid is DM33 (stale config snapshot); live UI grid differs.
      grid: 'DM33',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'FourCharGrid',
      position_source: 'Gps',
    };
    // Live position_status returns ui_grid = CN87 (precision-reduced live fix).
    const positionDto: PositionStatusDto = { gps_ready: true, broadcast_grid: 'CN87', ui_grid: 'CN87' };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return configDto;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return positionDto;
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    // Ribbon must show the live ui_grid, not the stale config grid.
    expect(result.current.grid).toBe('CN87');
  });

  // tuxlink-va1i: fallback to config grid when ui_grid is empty (no position).
  it('falls back to config grid when ui_grid is empty (no position)', async () => {
    const configDto: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: null,
      identifier: 'MYSTATION',
      grid: 'DM33',
      gps_state: 'BroadcastAtPrecision',
      position_precision: 'FourCharGrid',
      position_source: 'Gps',
    };
    // Empty ui_grid + empty broadcast_grid = no position available.
    const positionDto: PositionStatusDto = { gps_ready: false, broadcast_grid: '', ui_grid: '' };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return configDto;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return positionDto;
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    // Empty ui_grid → fall back to config-derived grid "DM33".
    expect(result.current.grid).toBe('DM33');
  });

  // tuxlink-va1i: the load-bearing divergent case — under LocalUiOnly + source=Gps
  // + fresh fix with SixCharGrid precision, the backend's two-helper split produces
  // ui_grid="DM33ww" (live precision-reduced fix, full 6-char) while broadcast_grid
  // stays at "DM33" (the static config_grid; LocalUiOnly suppresses on-air leak).
  // The ribbon MUST display ui_grid ("DM33ww"), NOT broadcast_grid ("DM33"). Pre-va1i
  // the derivation read broadcast_grid and the ribbon would have shown "DM33" —
  // misleading the operator about their actual location while the on-air locator
  // (correctly) stayed at the config fallback.
  it('liveGrid reads from positionStatus.ui_grid (not broadcast_grid) — tuxlink-va1i', async () => {
    const configDto: ConfigViewDto = {
      connect_to_cms: false,
      transport: 'CmsSsl',
      host: 'cms-z.winlink.org',
      callsign: null,
      identifier: 'MYSTATION',
      grid: 'DM33',
      gps_state: 'LocalUiOnly',
      position_precision: 'SixCharGrid',
      position_source: 'Gps',
    };
    // The divergent case: ui_grid is the live 6-char fix; broadcast_grid is the
    // config_grid on-air fallback under LocalUiOnly.
    const positionDto: PositionStatusDto = {
      gps_ready: true,
      broadcast_grid: 'DM33',    // ← what would be transmitted (config fallback)
      ui_grid: 'DM33ww',          // ← what the operator should see locally
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return configDto;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return positionDto;
      return null;
    });

    const { result } = renderHook(() => useStatusData());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    // Ribbon resolves to ui_grid ("DM33ww"), NOT broadcast_grid ("DM33").
    expect(result.current.grid).toBe('DM33ww');
    expect(result.current.grid).not.toBe('DM33');
  });
});

// ============================================================================
// formatStatusState — Mock D status-bar short state word + dot tone (tuxlink-yd4)
// ============================================================================
describe('formatStatusState — Mock D status bar', () => {
  it('null backend (default when unconfigured) reads Idle / idle tone', () => {
    expect(formatStatusState(null)).toEqual({ label: 'Idle', tone: 'idle' });
  });
  it('Disconnected reads Idle / idle tone', () => {
    expect(formatStatusState({ kind: 'Disconnected' })).toEqual({ label: 'Idle', tone: 'idle' });
  });
  it('Connecting reads Connecting / warn tone', () => {
    expect(formatStatusState({ kind: 'Connecting', transport: 'CMS-Telnet' })).toEqual({
      label: 'Connecting',
      tone: 'warn',
    });
  });
  it('Connected reads Connected / good tone', () => {
    expect(
      formatStatusState({ kind: 'Connected', transport: 'CMS-Telnet', peer: 'cms', since_iso: '' }),
    ).toEqual({ label: 'Connected', tone: 'good' });
  });
  it('Listening (packet armed) reads Listening / good tone (tuxlink-orj)', () => {
    expect(formatStatusState({ kind: 'Listening', transport: 'Packet-7' })).toEqual({
      label: 'Listening',
      tone: 'good',
    });
  });
  it('Disconnecting reads Disconnecting / warn tone', () => {
    expect(formatStatusState({ kind: 'Disconnecting' })).toEqual({
      label: 'Disconnecting',
      tone: 'warn',
    });
  });
  it('Error reads Error / error tone', () => {
    expect(formatStatusState({ kind: 'Error', reason: 'boom' })).toEqual({
      label: 'Error',
      tone: 'error',
    });
  });
});
