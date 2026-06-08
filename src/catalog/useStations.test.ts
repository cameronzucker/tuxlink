import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useStations } from './useStations';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useStations', () => {
  it('fetches + exposes gateways, sets loading false', async () => {
    vi.mocked(invoke).mockResolvedValue([
      {
        mode: 'ardop-hf',
        title: 't',
        parsedOk: true,
        raw: 'r',
        fetchedAtMs: 1000,
        gateways: [
          {
            channel: 'AI4Y.WINLINK',
            callsign: 'AI4Y',
            sysopName: null,
            grid: 'FM07CC',
            location: null,
            frequenciesKhz: [7101.6],
            lastUpdate: null,
            email: null,
            homepage: null,
          },
        ],
      },
    ]);
    const { result } = renderHook(() => useStations());
    act(() => result.current.fetch(['ardop-hf']));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.listings[0].gateways).toHaveLength(1);
    expect(result.current.error).toBeNull();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      'catalog_fetch_stations',
      expect.objectContaining({ modes: ['ardop-hf'] }),
    );
  });

  // The error path is `catch (e) => setError(catalogErrorMessage(e))` — trivial plumbing.
  // The real logic (extracting a message from every UiError wire shape) is unit-tested
  // directly in stationTypes.test.ts (catalogErrorMessage), which avoids a brittle
  // async-rejection test that fights vitest/act's fire-and-forget rejection tracking.
});
