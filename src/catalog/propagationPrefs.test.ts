import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import {
  readPropagationPrefs,
  writePropagationPrefs,
  ANTENNA_PRESET_OPTIONS,
  HEIGHT_GRID_M,
  isHeightVariable,
  DEFAULT_PROPAGATION_PREFS,
} from './propagationPrefs';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('readPropagationPrefs', () => {
  it('maps the snake_case wire shape to camelCase', async () => {
    vi.mocked(invoke).mockResolvedValue({ antenna_preset: 'base-vertical-radials', req_snr_db: 24, tx_power_w: 50, antenna_height_m: 12, ground_type: 'poor-soil', noise_environment: 'rural' } as unknown as never);
    const got = await readPropagationPrefs();
    expect(invoke).toHaveBeenCalledWith('propagation_prefs_read');
    expect(got).toEqual({ antennaPreset: 'base-vertical-radials', reqSnrDb: 24, txPowerW: 50, antennaHeightM: 12, groundType: 'poor-soil', noiseEnvironment: 'rural' });
  });
});

describe('writePropagationPrefs', () => {
  it('invokes with camelCase keys Tauri maps to the Rust snake_case params', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined as unknown as never);
    await writePropagationPrefs({ antennaPreset: 'efhw-sloper', reqSnrDb: 22, txPowerW: 100, antennaHeightM: 9, groundType: 'average', noiseEnvironment: 'residential' });
    expect(invoke).toHaveBeenCalledWith('propagation_prefs_write', {
      antennaPreset: 'efhw-sloper', reqSnrDb: 22, txPowerW: 100, antennaHeightM: 9, groundType: 'average', noiseEnvironment: 'residential',
    });
  });
});

describe('ANTENNA_PRESET_OPTIONS', () => {
  it('leads with the EFHW sloper default and covers the curated 8 presets', () => {
    expect(ANTENNA_PRESET_OPTIONS[0].value).toBe('efhw-sloper');
    expect(ANTENNA_PRESET_OPTIONS).toHaveLength(8);
    // Every value is unique.
    const values = ANTENNA_PRESET_OPTIONS.map((o) => o.value);
    expect(new Set(values).size).toBe(8);
  });

  it('drops the retired random-wire / magnetic-loop presets', () => {
    const values = ANTENNA_PRESET_OPTIONS.map((o) => o.value);
    expect(values).not.toContain('random-wire-unun');
    expect(values).not.toContain('magnetic-loop');
  });
});

describe('height grid + classification', () => {
  it('exposes the four-stop grid', () => {
    expect([...HEIGHT_GRID_M]).toEqual([2.5, 4, 6, 9]);
  });

  it('classifies horizontals as height-variable and verticals as fixed', () => {
    expect(isHeightVariable('efhw-sloper')).toBe(true);
    expect(isHeightVariable('beam-yagi')).toBe(true);
    expect(isHeightVariable('base-vertical-radials')).toBe(false);
    expect(isHeightVariable('mobile-hf-whip')).toBe(false);
    expect(isHeightVariable('unknown')).toBe(false);
  });
});

describe('DEFAULT_PROPAGATION_PREFS', () => {
  it('defaults ground to poor-soil (the Phase 1 single-ground model)', () => {
    expect(DEFAULT_PROPAGATION_PREFS.groundType).toBe('poor-soil');
  });
});

