// tuxlink-vnm5: a heard weather-symbol station with no valid readings must never
// silently render nothing (the "looks broken" report) or garbage. The card shows
// an honest state — "sensors offline" / "position-only" — plus the raw WX run for
// transparent inspection.
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { EnvStationCard } from './EnvStationCard';
import type { EnvStation } from './envStations';

function station(over: Partial<EnvStation>): EnvStation {
  return {
    call: 'WX7FGZ-2',
    project: '',
    seq: null,
    channels: [],
    bits: [],
    rain: null,
    wxStatus: null,
    rawWx: '',
    lastHeard: 1000,
    ...over,
  };
}

describe('EnvStationCard — honest no-data states', () => {
  it('shows a sensors-offline chip + the raw run rather than blank/garbage', () => {
    render(
      <EnvStationCard
        station={station({ wxStatus: 'sensorsOffline', rawWx: '767/255g255t200r000p000P000h00b00000' })}
        now={1000}
      />,
    );
    expect(screen.getByTestId('env-wx-offline-WX7FGZ-2')).toBeInTheDocument();
    expect(screen.getByTestId('env-wxnote-WX7FGZ-2')).toHaveTextContent(/sensors offline/i);
    // The raw bytes are preserved + visible (operator: don't throw the data out).
    expect(screen.getByTestId('env-wxraw-WX7FGZ-2')).toHaveTextContent('767/255g255t200');
  });

  it('shows a position-only marker (with its name) for a weather-symbol name beacon', () => {
    render(
      <EnvStationCard
        station={station({ call: 'KA7WSB-2', wxStatus: 'positionOnly', project: 'NPS_003_Chiminea' })}
        now={1000}
      />,
    );
    expect(screen.getByTestId('env-wx-positiononly-KA7WSB-2')).toBeInTheDocument();
    expect(screen.getByTestId('env-wxnote-KA7WSB-2')).toHaveTextContent(/no measurements/i);
    expect(screen.getByTestId('env-card-KA7WSB-2')).toHaveTextContent('NPS_003_Chiminea');
  });

  it('renders normal readings (no offline/position-only note) when valid', () => {
    render(
      <EnvStationCard
        station={station({
          wxStatus: 'readings',
          channels: [
            { key: 'temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: 72, scaled: true, history: [] },
          ],
        })}
        now={1000}
      />,
    );
    expect(screen.queryByTestId('env-wxnote-WX7FGZ-2')).not.toBeInTheDocument();
  });
});
