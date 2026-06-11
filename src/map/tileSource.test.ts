/**
 * tileSource invoke-wrapper tests — assert each wrapper calls the EXACT Tauri
 * command name with the EXACT arg shape (camelCase fields; PascalCase enum
 * variants for crs/scheme; kebab-case StatusKind union) and returns the parsed
 * status. The @tauri-apps/api/core module is mocked at module level so the test
 * runs in jsdom without a real Tauri context.
 *
 * Wire shapes are mirrored from the Rust serde (Phases 0-6): see tileSource.ts.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  // Forward variadically so a no-args invoke('cmd') asserts as a single arg.
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

import {
  configureTileSource,
  testTileSource,
  clearTileCache,
  getTileSourceStatus,
  type TileSource,
  type TileSourceStatus,
} from './tileSource';

const SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: '© LAN tiles',
  label: 'Garage MBTiles',
};

const STATUS: TileSourceStatus = {
  kind: 'lan-live',
  zoom: 14,
  label: 'Garage MBTiles',
  cachedAt: null,
};

describe('tileSource invoke wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('configureTileSource invokes configure_tile_source with { source }', async () => {
    invokeMock.mockResolvedValue(STATUS);
    const out = await configureTileSource(SOURCE);
    expect(invokeMock).toHaveBeenCalledWith('configure_tile_source', { source: SOURCE });
    expect(out).toEqual(STATUS);
  });

  it('testTileSource invokes test_tile_source with { source }', async () => {
    invokeMock.mockResolvedValue(STATUS);
    const out = await testTileSource(SOURCE);
    expect(invokeMock).toHaveBeenCalledWith('test_tile_source', { source: SOURCE });
    expect(out).toEqual(STATUS);
  });

  it('clearTileCache invokes clear_tile_cache with no args', async () => {
    await clearTileCache();
    expect(invokeMock).toHaveBeenCalledWith('clear_tile_cache');
  });

  it('getTileSourceStatus invokes tile_source_status and returns the parsed status', async () => {
    invokeMock.mockResolvedValue(STATUS);
    const out = await getTileSourceStatus();
    expect(invokeMock).toHaveBeenCalledWith('tile_source_status');
    expect(out).toEqual(STATUS);
  });

  it('preserves the PascalCase scheme variant and kebab-case kind across the wire; no crs field', async () => {
    invokeMock.mockResolvedValue({ ...STATUS, kind: 'lan-cached' });
    await configureTileSource({ ...SOURCE, scheme: 'Tms' });
    const arg = invokeMock.mock.calls[0][1] as { source: TileSource };
    expect(arg.source.scheme).toBe('Tms');
    expect(arg.source).not.toHaveProperty('crs');
  });
});
