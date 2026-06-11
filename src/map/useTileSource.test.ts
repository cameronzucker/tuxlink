/**
 * useTileSource hook tests (Task 6, tuxlink-7h2m).
 *
 * Verifies that the hook:
 *   - Reads the persisted tile source from config_read (.map_tile_source)
 *   - Fetches the live status via tile_source_status
 *   - Returns { source, status } when source is present AND status.kind is
 *     one of lan-live / lan-cached / partial
 *   - Returns null when map_tile_source is null/absent
 *   - Returns null when status.kind is bundled / unreachable / incompatible
 *   - Returns null on any invoke error (never throws)
 *
 * Follows the exact invoke-mock pattern from tileSource.test.ts and
 * PositionPickerOverlay.test.tsx.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

import { useTileSource } from './useTileSource';
import type { TileSource, TileSourceStatus } from './tileSource';

const SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: '© LAN tiles',
  label: 'Geographica',
};

function makeLanLiveStatus(): TileSourceStatus {
  return { kind: 'lan-live', zoom: 16, label: 'Geographica', cachedAt: null };
}

function setupInvoke(mapTileSource: TileSource | null, status: TileSourceStatus): void {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { map_tile_source: mapTileSource };
    if (cmd === 'tile_source_status') return status;
    return undefined;
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('useTileSource', () => {
  it('returns { source, status } when map_tile_source is set and status is lan-live', async () => {
    const status = makeLanLiveStatus();
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current).toEqual({ source: SOURCE, status });
  });

  it('returns { source, status } when status is lan-cached', async () => {
    const status: TileSourceStatus = { kind: 'lan-cached', zoom: 14, label: 'Geographica', cachedAt: '2026-06-10T00:00:00Z' };
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current).toEqual({ source: SOURCE, status });
  });

  it('returns { source, status } when status is partial', async () => {
    const status: TileSourceStatus = { kind: 'partial', zoom: 12, label: 'Geographica', cachedAt: null };
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current).toEqual({ source: SOURCE, status });
  });

  it('returns null when map_tile_source is null (no LAN source configured)', async () => {
    const status: TileSourceStatus = { kind: 'bundled', zoom: 3, label: null, cachedAt: null };
    setupInvoke(null, status);
    const { result } = renderHook(() => useTileSource());
    // Give the async effect time to settle; result should stay null
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('config_read'),
    );
    expect(result.current).toBeNull();
  });

  it('returns null when status is bundled (no live tile source)', async () => {
    const status: TileSourceStatus = { kind: 'bundled', zoom: 3, label: null, cachedAt: null };
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('tile_source_status'),
    );
    expect(result.current).toBeNull();
  });

  it('returns null when status is unreachable', async () => {
    const status: TileSourceStatus = { kind: 'unreachable', zoom: 0, label: 'Geographica', cachedAt: null };
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('tile_source_status'),
    );
    expect(result.current).toBeNull();
  });

  it('returns null when status is incompatible', async () => {
    const status: TileSourceStatus = { kind: 'incompatible', zoom: 0, label: 'Geographica', cachedAt: null };
    setupInvoke(SOURCE, status);
    const { result } = renderHook(() => useTileSource());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('tile_source_status'),
    );
    expect(result.current).toBeNull();
  });

  it('returns null (never throws) when config_read rejects', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') throw new Error('IPC error');
      return undefined;
    });
    const { result } = renderHook(() => useTileSource());
    // Allow the async effect to settle; null is the safe fallback
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('config_read'),
    );
    expect(result.current).toBeNull();
  });

  it('returns null (never throws) when tile_source_status rejects', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { map_tile_source: SOURCE };
      if (cmd === 'tile_source_status') throw new Error('IPC error');
      return undefined;
    });
    const { result } = renderHook(() => useTileSource());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('tile_source_status'),
    );
    expect(result.current).toBeNull();
  });
});
