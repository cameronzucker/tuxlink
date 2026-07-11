import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { refreshOffair, readSnapshot, type WwvRefreshOutcome, type SolarSnapshot } from './wwvApi';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('refreshOffair', () => {
  it('invokes wwv_offair_refresh with camelCase nowMs and returns the snake_case outcome verbatim', async () => {
    const outcome: WwvRefreshOutcome = {
      updated: true,
      indices: { sfi: 150, a_index: 8, k_index: 2 },
      source: 'rf-wwv-voice',
      no_copy: false,
    };
    vi.mocked(invoke).mockResolvedValue(outcome as unknown as never);

    const got = await refreshOffair(1_783_512_000_000);

    expect(invoke).toHaveBeenCalledWith('wwv_offair_refresh', { nowMs: 1_783_512_000_000 });
    expect(got).toEqual(outcome);
  });

  it('surfaces no_copy on a failed decode', async () => {
    const outcome: WwvRefreshOutcome = { updated: false, indices: null, source: 'rf-wwv-voice', no_copy: true };
    vi.mocked(invoke).mockResolvedValue(outcome as unknown as never);

    const got = await refreshOffair(123);

    expect(got.no_copy).toBe(true);
  });
});

describe('readSnapshot', () => {
  it('invokes wwv_offair_snapshot_read with no args and returns the snapshot verbatim', async () => {
    const snapshot: SolarSnapshot = {
      indices: { sfi: 150 },
      updated_at_ms: 1_783_512_000_000,
      source: 'rf-wwv-voice',
      forecast_updated: true,
    };
    vi.mocked(invoke).mockResolvedValue(snapshot as unknown as never);

    const got = await readSnapshot();

    expect(invoke).toHaveBeenCalledWith('wwv_offair_snapshot_read');
    expect(got).toEqual(snapshot);
  });

  it('passes through null (no snapshot persisted yet)', async () => {
    vi.mocked(invoke).mockResolvedValue(null as unknown as never);

    const got = await readSnapshot();

    expect(got).toBeNull();
  });
});
