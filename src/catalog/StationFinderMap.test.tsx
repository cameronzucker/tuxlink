import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
vi.mock('../map/useTileSource', () => ({ useTileSource: vi.fn() }));
import { resetMapMock } from '../map/testMapMock';
import { useTileSource } from '../map/useTileSource';
import type { TileSource, TileSourceStatus } from '../map/tileSource';

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

const LAN_SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: null,
  label: 'LAN source',
};
const LAN_STATUS: TileSourceStatus = { kind: 'lan-live', zoom: 16, label: 'LAN source', cachedAt: null };

beforeEach(() => {
  resetMapMock();
  vi.mocked(useTileSource).mockReturnValue(null);
});

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

  // Task 7: validate tile source is wired through to BaseMap
  it('passes tileSource to BaseMap when useTileSource returns a lan-live source', () => {
    vi.mocked(useTileSource).mockReturnValue({ source: LAN_SOURCE, status: LAN_STATUS });
    render(<StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    // When tileSource is tile-backed, BaseMap renders TileLayerBridge → leaflet-tilelayer
    expect(screen.getByTestId('leaflet-tilelayer')).toBeInTheDocument();
  });

  it('renders no TileLayer when useTileSource returns null (offline fallback)', () => {
    vi.mocked(useTileSource).mockReturnValue(null);
    render(<StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    expect(screen.queryByTestId('leaflet-tilelayer')).toBeNull();
  });
});
