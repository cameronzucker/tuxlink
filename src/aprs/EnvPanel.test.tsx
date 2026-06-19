import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { EnvPanel } from './EnvPanel';
import { applyWeather, applyTelemetry, STALE_AFTER_MS, type EnvStation } from './envStations';
import type { WeatherReportDto, InboundTelemetryDto } from './aprsTypes';

function wx(p: Partial<WeatherReportDto>): WeatherReportDto {
  return {
    station: 'KE7ABC-13',
    windDirectionDeg: null, windSpeedMph: null, windGustMph: null,
    temperatureF: null, humidityPct: null, pressureHpa: null,
    rain1hIn: null, rain24hIn: null, rainSinceMidnightIn: null,
    luminosityWm2: null, snowIn: null, comment: '',
    ...p,
  };
}
function tlm(p: Partial<InboundTelemetryDto>): InboundTelemetryDto {
  return { station: 'W7DIGI-2', seq: null, analog: [], digital: [], project: '', comment: '', ...p };
}

const NOW = 1_000_000;

describe('EnvPanel — empty + populated', () => {
  it('shows an honest empty state when no stations have been heard', () => {
    render(<EnvPanel stations={[]} now={NOW} />);
    expect(screen.getByTestId('env-empty')).toBeInTheDocument();
    expect(screen.queryByTestId(/^env-card-/)).not.toBeInTheDocument();
  });

  it('renders one card per heard station, labeled by callsign', () => {
    const s1 = applyWeather(undefined, wx({ station: 'KE7ABC-13', temperatureF: 52 }), NOW);
    const s2 = applyTelemetry(undefined, tlm({ station: 'W7DIGI-2' }), NOW);
    render(<EnvPanel stations={[s1, s2]} now={NOW} />);
    expect(screen.getByTestId('env-card-KE7ABC-13')).toBeInTheDocument();
    expect(screen.getByTestId('env-card-W7DIGI-2')).toBeInTheDocument();
    expect(screen.getByText('KE7ABC-13')).toBeInTheDocument();
  });

  it('scrolls the focused station card into view (ni5b)', () => {
    const scroll = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { value: scroll, writable: true, configurable: true });
    const s1 = applyWeather(undefined, wx({ station: 'W7AAA', temperatureF: 50 }), NOW);
    const s2 = applyWeather(undefined, wx({ station: 'W7BBB', temperatureF: 60 }), NOW);
    render(<EnvPanel stations={[s1, s2]} focusCall="W7BBB" now={NOW} />);
    expect(screen.getByTestId('env-card-W7BBB')).toBeInTheDocument();
    expect(scroll).toHaveBeenCalled();
  });
});

describe('EnvStationCard — source-reactive rendering', () => {
  function build(): EnvStation {
    let s = applyWeather(
      undefined,
      wx({ station: 'KE7ABC-13', windDirectionDeg: 270, windSpeedMph: 8, windGustMph: 15, temperatureF: 52, humidityPct: 78, pressureHpa: 1013, rain1hIn: 0.04, rain24hIn: 0.12, comment: 'Davis Vantage Pro' }),
      NOW - 2000,
    );
    s = applyWeather(s, wx({ station: 'KE7ABC-13', windDirectionDeg: 270, windSpeedMph: 8, temperatureF: 53, humidityPct: 77, pressureHpa: 1012 }), NOW);
    return s;
  }

  it('renders a wind compass when wind direction is present', () => {
    render(<EnvPanel stations={[build()]} now={NOW} />);
    expect(screen.getByTestId('env-compass')).toBeInTheDocument();
  });

  it('renders a graded chart for temperature, humidity, and pressure', () => {
    render(<EnvPanel stations={[build()]} now={NOW} />);
    expect(screen.getByTestId('env-chart-wx:temperature')).toBeInTheDocument();
    expect(screen.getByTestId('env-chart-wx:humidity')).toBeInTheDocument();
    expect(screen.getByTestId('env-chart-wx:pressure')).toBeInTheDocument();
  });

  it('shows a computed pressure rise/fall trend', () => {
    render(<EnvPanel stations={[build()]} now={NOW} />);
    // pressure fell 1013 → 1012, so the trend marker reads falling
    expect(screen.getByTestId('env-pressure-trend')).toHaveTextContent(/fall/i);
  });

  it('renders rain totals as a dedicated block, not a chart', () => {
    render(<EnvPanel stations={[build()]} now={NOW} />);
    expect(screen.getByTestId('env-rain')).toHaveTextContent('0.04');
    expect(screen.queryByTestId('env-chart-rain')).not.toBeInTheDocument();
  });

  it('marks an unscaled (no-EQNS) telemetry channel as a raw count, never a fake unit', () => {
    const s = applyTelemetry(undefined, tlm({ station: 'N0CALL-7', analog: [{ name: 'A1', unit: '', raw: 199, value: 199, scaled: false }] }), NOW);
    render(<EnvPanel stations={[s]} now={NOW} />);
    expect(screen.getByTestId('env-card-N0CALL-7')).toHaveTextContent(/raw/i);
  });

  it('renders digital bits as LED pills carrying their on/off state', () => {
    const s = applyTelemetry(undefined, tlm({ station: 'W7DIGI-2', digital: [{ name: 'Fan', value: true, sense: true }, { name: 'Door', value: false, sense: true }] }), NOW);
    render(<EnvPanel stations={[s]} now={NOW} />);
    const bits = screen.getByTestId('env-bits');
    expect(bits).toHaveTextContent('Fan');
    expect(bits).toHaveTextContent('Door');
    expect(bits.querySelector('.env-bit.is-on')).toBeTruthy();
  });

  it('dims a stale station that has not been heard within the stale window', () => {
    const s = applyWeather(undefined, wx({ station: 'OLD', temperatureF: 40 }), NOW - STALE_AFTER_MS - 1);
    render(<EnvPanel stations={[s]} now={NOW} />);
    expect(screen.getByTestId('env-card-OLD').className).toMatch(/is-stale/);
  });
});
