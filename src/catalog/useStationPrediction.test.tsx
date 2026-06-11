import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useStationPrediction } from './useStationPrediction';
import type { Station } from './stationModel';

const station: Station = {
  baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug', location: 'Wickenburg, AZ',
  modes: ['vara-hf'], fetchedAtMs: 1,
  channels: [
    { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
    { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
  ],
};

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useStationPrediction', () => {
  it('predicts over the station HF dials (deduped, VHF excluded) and returns ok', async () => {
    // Gate on cmd: the test runner makes a stray no-arg call to the mock during
    // cleanup; returning undefined for anything but our command keeps it inert.
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'propagation_predict_path'
        ? ({
            bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
            channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.8), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
          } as unknown as never)
        : (undefined as unknown as never),
    );
    const { result } = renderHook(() => useStationPrediction('DM43bp', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('ok'));
    const arg = vi.mocked(invoke).mock.calls[0][1] as { frequenciesKhz: number[] };
    expect(arg.frequenciesKhz.slice().sort((a, b) => a - b)).toEqual([3590, 7103]); // VHF dropped
    expect(result.current.prediction?.bearingDeg).toBe(318);
  });

  it('reports unavailable (not error) when the engine is not bundled', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'propagation_predict_path') throw { kind: 'Unavailable', reason: 'voacapl not bundled' };
      return undefined as unknown as never;
    });
    const { result } = renderHook(() => useStationPrediction('DM43bp', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('unavailable'));
    expect(result.current.prediction).toBeNull();
  });

  it('reports no-location when the operator grid is empty', async () => {
    const { result } = renderHook(() => useStationPrediction('', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('no-location'));
    expect(invoke).not.toHaveBeenCalled();
  });

  it('is idle with no station selected', async () => {
    const { result } = renderHook(() => useStationPrediction('DM43bp', null), { wrapper: wrap() });
    expect(result.current.status).toBe('idle');
    expect(invoke).not.toHaveBeenCalled();
  });
});
