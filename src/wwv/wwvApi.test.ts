import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import {
  refreshOffair,
  readSnapshot,
  readClip,
  manualIngest,
  catConfigured,
  discardClip,
  type WwvRefreshOutcome,
  type SolarSnapshot,
} from './wwvApi';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('refreshOffair', () => {
  it('invokes wwv_offair_refresh with camelCase nowMs and returns the snake_case outcome verbatim', async () => {
    const outcome: WwvRefreshOutcome = {
      updated: true,
      indices: { sfi: 150, a_index: 8, k_index: 2 },
      source: 'rf-wwv-voice',
      no_copy: false,
      wav_path: null,
    };
    vi.mocked(invoke).mockResolvedValue(outcome as unknown as never);

    const got = await refreshOffair(1_783_512_000_000);

    expect(invoke).toHaveBeenCalledWith('wwv_offair_refresh', { nowMs: 1_783_512_000_000 });
    expect(got).toEqual(outcome);
  });

  it('surfaces no_copy on a failed decode', async () => {
    const outcome: WwvRefreshOutcome = {
      updated: false,
      indices: null,
      source: 'rf-wwv-voice',
      no_copy: true,
      wav_path: '/tmp/clip.wav',
    };
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

describe('readClip', () => {
  it('invokes wwv_offair_read_clip with the path and returns a Uint8Array', async () => {
    vi.mocked(invoke).mockResolvedValue([1, 2, 3] as unknown as never);

    const got = await readClip('/tmp/wwv-clip.wav');

    expect(invoke).toHaveBeenCalledWith('wwv_offair_read_clip', { path: '/tmp/wwv-clip.wav' });
    expect(got).toBeInstanceOf(Uint8Array);
    expect(Array.from(got)).toEqual([1, 2, 3]);
  });
});

describe('manualIngest', () => {
  it('invokes wwv_offair_manual_ingest with camelCase args and returns the outcome verbatim', async () => {
    const outcome: WwvRefreshOutcome = {
      updated: true,
      indices: { sfi: 133 },
      source: 'rf-wwv-manual',
      no_copy: false,
      wav_path: null,
    };
    vi.mocked(invoke).mockResolvedValue(outcome as unknown as never);

    const got = await manualIngest(133, 5, null, 1_783_512_000_000);

    expect(invoke).toHaveBeenCalledWith('wwv_offair_manual_ingest', {
      sfi: 133,
      aIndex: 5,
      kIndex: null,
      nowMs: 1_783_512_000_000,
    });
    expect(got).toEqual(outcome);
  });
});

describe('catConfigured', () => {
  it('invokes wwv_offair_cat_configured with no args and returns the boolean verbatim', async () => {
    vi.mocked(invoke).mockResolvedValue(true as unknown as never);

    const got = await catConfigured();

    expect(invoke).toHaveBeenCalledWith('wwv_offair_cat_configured');
    expect(got).toBe(true);
  });
});

describe('discardClip', () => {
  it('invokes wwv_offair_discard_clip with the path', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined as unknown as never);

    await discardClip('/tmp/wwv-clip.wav');

    expect(invoke).toHaveBeenCalledWith('wwv_offair_discard_clip', { path: '/tmp/wwv-clip.wav' });
  });
});
