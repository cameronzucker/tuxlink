import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
import { resetMapMock } from '../map/testMapMock';

import { StationFinderMap } from './StationFinderMap';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

const stations: Station[] = [
  { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'K0ABC', grid: 'EN34', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
];
const tiers = new Map([
  [stationKey(stations[0]), 'good' as const],
  [stationKey(stations[1]), 'skip' as const],
]);

beforeEach(() => resetMapMock());

describe('StationFinderMap', () => {
  it('renders a pin per station with a reach-tier class', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={() => {}} />);
    const pins = screen.getAllByTestId('station-pin');
    expect(pins).toHaveLength(2);
    expect(pins[0].className).toMatch(/good/);
    expect(pins[1].className).toMatch(/skip/);
  });

  it('selects a station when its pin is clicked', () => {
    const onSelect = vi.fn();
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={onSelect} />);
    fireEvent.click(screen.getAllByTestId('station-pin')[0]);
    expect(onSelect).toHaveBeenCalledWith(stations[0]);
  });

  it('renders an untiered pin when no tier is known (prediction unavailable)', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    const pins = screen.getAllByTestId('station-pin');
    expect(pins[0].className).toMatch(/untiered/);
  });

  it('renders the operator "you" pin', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={() => {}} />);
    expect(screen.getByTestId('me-pin')).toBeTruthy();
  });
});
