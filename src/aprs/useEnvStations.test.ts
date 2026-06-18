import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useEnvStations } from './useEnvStations';
import type { WeatherReportDto, InboundTelemetryDto } from './aprsTypes';

// Capture the per-event callbacks the hook registers, keyed by event name, so a
// test can drive `aprs-weather:new` / `aprs-telemetry:new` independently.
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
const listenMock = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: (e: { payload: unknown }) => void) => listenMock(event, cb),
}));

function wx(partial: Partial<WeatherReportDto>): WeatherReportDto {
  return {
    station: 'KE7ABC-13',
    windDirectionDeg: null, windSpeedMph: null, windGustMph: null,
    temperatureF: null, humidityPct: null, pressureHpa: null,
    rain1hIn: null, rain24hIn: null, rainSinceMidnightIn: null,
    luminosityWm2: null, snowIn: null, comment: '',
    ...partial,
  };
}
function tlm(partial: Partial<InboundTelemetryDto>): InboundTelemetryDto {
  return { station: 'W7DIGI-2', seq: null, analog: [], digital: [], project: '', comment: '', ...partial };
}

describe('useEnvStations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Deterministic, monotonically-increasing clock so "most-recently-heard"
    // ordering is unambiguous regardless of how fast the test dispatches frames.
    let clock = 1000;
    vi.spyOn(Date, 'now').mockImplementation(() => (clock += 1000));
    for (const k of Object.keys(handlers)) delete handlers[k];
    listenMock.mockImplementation((event: string, cb: (e: { payload: unknown }) => void) => {
      handlers[event] = cb;
      return Promise.resolve(() => {});
    });
  });

  it('subscribes to both the weather and telemetry events', async () => {
    renderHook(() => useEnvStations());
    await waitFor(() => {
      expect(listenMock).toHaveBeenCalledWith('aprs-weather:new', expect.any(Function));
      expect(listenMock).toHaveBeenCalledWith('aprs-telemetry:new', expect.any(Function));
    });
  });

  it('accumulates a heard weather station into the list', async () => {
    const { result } = renderHook(() => useEnvStations());
    await waitFor(() => expect(handlers['aprs-weather:new']).toBeDefined());
    act(() => handlers['aprs-weather:new']({ payload: wx({ station: 'KE7ABC-13', temperatureF: 52 }) }));
    expect(result.current.stations).toHaveLength(1);
    expect(result.current.stations[0].call).toBe('KE7ABC-13');
    expect(result.current.stations[0].channels.find((c) => c.kind === 'temperature')?.value).toBe(52);
  });

  it('merges a weather and a telemetry frame from the same callsign into one station', async () => {
    const { result } = renderHook(() => useEnvStations());
    await waitFor(() => expect(handlers['aprs-weather:new']).toBeDefined());
    act(() => handlers['aprs-weather:new']({ payload: wx({ station: 'N0AA', temperatureF: 52 }) }));
    act(() =>
      handlers['aprs-telemetry:new']({
        payload: tlm({ station: 'N0AA', analog: [{ name: 'Vbat', unit: 'V', raw: 220, value: 13.6, scaled: true }] }),
      }),
    );
    expect(result.current.stations).toHaveLength(1);
    const kinds = result.current.stations[0].channels.map((c) => c.kind).sort();
    expect(kinds).toEqual(['generic', 'temperature']);
  });

  it('orders stations most-recently-heard first', async () => {
    const { result } = renderHook(() => useEnvStations());
    await waitFor(() => expect(handlers['aprs-weather:new']).toBeDefined());
    act(() => handlers['aprs-weather:new']({ payload: wx({ station: 'OLD', temperatureF: 1 }) }));
    act(() => handlers['aprs-weather:new']({ payload: wx({ station: 'NEW', temperatureF: 2 }) }));
    expect(result.current.stations.map((s) => s.call)).toEqual(['NEW', 'OLD']);
  });
});
