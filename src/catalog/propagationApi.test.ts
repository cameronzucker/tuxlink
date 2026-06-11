import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { predictPath, isUnavailable, type PathPrediction } from './propagationApi';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('predictPath', () => {
  it('invokes propagation_predict_path with camelCase args and returns the prediction', async () => {
    const resp: PathPrediction = {
      bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
      channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.8), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
    };
    vi.mocked(invoke).mockResolvedValue(resp as unknown as never);
    const got = await predictPath('DM43bp', 'DM34oa', [7103, 14103]);
    expect(invoke).toHaveBeenCalledWith('propagation_predict_path', {
      txGrid: 'DM43bp', rxGrid: 'DM34oa', frequenciesKhz: [7103, 14103],
    });
    expect(got).toEqual(resp);
  });

  it('caps at 11 frequencies (backend rejects more)', async () => {
    vi.mocked(invoke).mockResolvedValue({ channels: [] } as unknown as never);
    const many = Array.from({ length: 20 }, (_, i) => 7000 + i);
    await predictPath('DM43bp', 'DM34oa', many);
    const arg = vi.mocked(invoke).mock.calls[0][1] as { frequenciesKhz: number[] };
    expect(arg.frequenciesKhz).toHaveLength(11);
  });
});

describe('isUnavailable', () => {
  it('recognises the UiError::Unavailable variant', () => {
    expect(isUnavailable({ kind: 'Unavailable', reason: 'voacapl not bundled' })).toBe(true);
    expect(isUnavailable({ kind: 'Rejected', reason: 'bad grid' })).toBe(false);
    expect(isUnavailable(new Error('boom'))).toBe(false);
    expect(isUnavailable(null)).toBe(false);
  });
});
