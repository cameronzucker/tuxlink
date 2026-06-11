import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-mercator-2048.png', () => ({ default: '/world-mercator-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
vi.mock('../map/useTileSource', () => ({ useTileSource: vi.fn() }));
import { resetMapMock, getMockMap } from '../map/testMapMock';
import { gridToLatLon } from '../forms/position/maidenhead';
import { useTileSource } from '../map/useTileSource';
import type { TileSource, TileSourceStatus } from '../map/tileSource';

import { StationFinderMap, stationPinIcon } from './StationFinderMap';
import type { Station } from './stationModel';

// NOTE: real-pin RENDERING + click are validated by browser smoke, not here.
// react-leaflet draws pins as L.divIcon markers; the test map mock renders
// <Marker> as a bare div and cannot represent divIcon HTML or eventHandlers
// (relying on it is exactly what shipped the broken map — tuxlink-ku2b). These
// tests cover the icon-building LOGIC (tier → colour class + size) and that the
// component mounts the right number of markers without crashing.

const stations: Station[] = [
  { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'K0ABC', grid: 'EN34', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'NOGRID', grid: '', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
];

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

describe('stationPinIcon (reachability colour/size logic)', () => {
  it('encodes the tier as a colour class + size in the icon HTML', () => {
    const good = stationPinIcon('good', false, 'N0DAJ') as unknown as { html: string; iconSize: [number, number] };
    expect(good.html).toMatch(/station-finder__pindot--good/);
    expect(good.iconSize[0]).toBe(20);

    const skip = stationPinIcon('skip', false, 'X') as unknown as { html: string; iconSize: [number, number] };
    expect(skip.html).toMatch(/station-finder__pindot--skip/);
    expect(skip.iconSize[0]).toBe(10);
  });

  it('falls back to an untiered dot when no tier is known', () => {
    const u = stationPinIcon(undefined, false, 'X') as unknown as { html: string };
    expect(u.html).toMatch(/station-finder__pindot--untiered/);
  });

  it('marks the selected pin', () => {
    const sel = stationPinIcon('good', true, 'X') as unknown as { html: string };
    expect(sel.html).toMatch(/is-selected/);
  });
});

describe('StationFinderMap', () => {
  it('mounts a marker per placeable station + the operator pin, dropping gridless', () => {
    render(
      <StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    // 2 placeable stations (NOGRID dropped) + 1 operator "you" pin = 3 markers.
    expect(screen.getAllByTestId('leaflet-marker')).toHaveLength(3);
  });

  it('omits the operator pin when no grid is set', () => {
    render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    // 2 placeable stations, no "you" pin.
    expect(screen.getAllByTestId('leaflet-marker')).toHaveLength(2);
  });

  // Regression: react-leaflet's MapContainer center/zoom are mount-only props,
  // but StationFinderPanel loads the operator grid asynchronously (config_read)
  // AFTER the map mounts. Without an imperative recenter the map stays parked at
  // [0,0] (mid-Atlantic). The component must setView() to the operator latlon
  // once a grid is present, and re-setView when the grid changes.
  it('recenters the map on the operator grid once it is provided', () => {
    const { rerender } = render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    // No grid yet → no imperative recenter.
    expect(getMockMap().setView).not.toHaveBeenCalled();

    // Grid resolves (as config_read would) → recenter to the operator location.
    rerender(
      <StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const me = gridToLatLon('DM43bp')!;
    expect(getMockMap().setView).toHaveBeenCalledWith([me.lat, me.lon], expect.any(Number));
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
