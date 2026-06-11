import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useReachabilityMap, stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

function station(call: string, grid: string, khz: number[]): Station {
  return { baseCallsign: call, grid, sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1,
    channels: khz.map((f) => ({ mode: 'vara-hf' as const, frequencyKhz: f, band: f < 8000 ? '40m' as const : '20m' as const })) };
}

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useReachabilityMap', () => {
  const stations = [station('N0DAJ', 'DM34oa', [7103]), station('K0ABC', 'EN34', [7103])];

  it('assigns a tier per station from current-hour REL on the selected band', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd !== 'propagation_predict_path') return undefined as unknown as never;
      const rx = (args as { rxGrid: string }).rxGrid;
      const rel = rx === 'DM34oa' ? 0.86 : 0.12; // near=good, far=skip on 40m
      return { bearingDeg: 0, distanceKm: 1, ssn: 118, year: 2026, month: 6,
        channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(rel), snrByHour: Array(24).fill(5), mufdayByHour: Array(24).fill(0.5) }] } as unknown as never;
    });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.available).toBe(true));
    expect(result.current.tiers.get(stationKey(stations[0]))).toBe('good');
    expect(result.current.tiers.get(stationKey(stations[1]))).toBe('skip');
  });

  it('marks unavailable + empty tiers when the engine is not bundled', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'propagation_predict_path') throw { kind: 'Unavailable', reason: 'no voacapl' };
      return undefined as unknown as never;
    });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.available).toBe(false);
    expect(result.current.tiers.size).toBe(0);
  });

  it('always provides distances regardless of prediction availability', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'propagation_predict_path') throw { kind: 'Unavailable' };
      return undefined as unknown as never;
    });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.distances.get(stationKey(stations[0]))).toBeGreaterThan(0);
  });
});
