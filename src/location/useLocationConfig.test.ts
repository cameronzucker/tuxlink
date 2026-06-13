import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useLocationConfig } from './useLocationConfig';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useLocationConfig', () => {
  it('seeds grid + source from config_read', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'EM75', position_source: 'Manual' } as never;
      if (cmd === 'position_status') return { gps_ready: false, ui_grid: '', fix_lat: null, fix_lon: null } as never;
      return undefined as never;
    });
    const { result } = renderHook(() => useLocationConfig());
    await waitFor(() => expect(result.current.grid).toBe('EM75'));
    expect(result.current.selectedSource).toBe('manual');
  });

  it('exposes live gps_ready + fix coords + uiGrid from position_status', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'EM75', position_source: 'Gps' } as never;
      if (cmd === 'position_status')
        return { gps_ready: true, ui_grid: 'EM75km', fix_lat: 36.1, fix_lon: -86.8 } as never;
      return undefined as never;
    });
    const { result } = renderHook(() => useLocationConfig());
    await waitFor(() => expect(result.current.gpsReady).toBe(true));
    expect(result.current.fixLat).toBeCloseTo(36.1);
    expect(result.current.fixLon).toBeCloseTo(-86.8);
    expect(result.current.uiGrid).toBe('EM75km');
  });

  it('treats absent fix coords as null', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: null, position_source: 'Manual' } as never;
      if (cmd === 'position_status') return { gps_ready: false, ui_grid: '' } as never; // no fix_lat/lon
      return undefined as never;
    });
    const { result } = renderHook(() => useLocationConfig());
    await waitFor(() => expect(result.current.uiGrid).toBe(''));
    expect(result.current.fixLat).toBeNull();
    expect(result.current.fixLon).toBeNull();
  });
});
