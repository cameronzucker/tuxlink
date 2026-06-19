import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, act, screen, fireEvent } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { gridToLatLon } from '../forms/position/maidenhead';
import { AprsPositionsMap, ambiguityRadiusMeters } from './AprsPositionsMap';
import type { HeardPosition } from './aprsTypes';

// MapLibre re-expression (mirrors StationFinderMap.test.tsx): pins are GeoJSON
// circle-layer features driven through the global maplibre-gl test double, not
// Leaflet markers. These tests prove the source/feature wiring (one feature per
// heard station, callsign label property). Render fidelity is grim-only.

const positions: HeardPosition[] = [
  { call: 'KK6XYZ', lat: 49.05, lon: -72.03, symbolTable: '/', symbolCode: '-', comment: 'Hello', at: 1, ambiguity: 0 },
  { call: 'W7ABC', lat: 40.0, lon: -100.0, symbolTable: '/', symbolCode: '>', comment: 'Mobile', at: 2, ambiguity: 0 },
];

interface PinFeature {
  properties: { call: string; comment: string; ambiguity: number };
  geometry: { coordinates: [number, number] };
}

interface CircleFeature {
  properties: { call: string; ambiguity: number };
  geometry: { type: string };
}

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

function sourceData(map: MapLibreMock, id: string): { features: PinFeature[] } {
  return (map.getSource(id) as { data: { features: PinFeature[] } }).data;
}

function circleData(map: MapLibreMock, id: string): { features: CircleFeature[] } {
  return (map.getSource(id) as { data: { features: CircleFeature[] } }).data;
}

describe('AprsPositionsMap', () => {
  it('renders a testid container', () => {
    const { getByTestId } = render(<AprsPositionsMap positions={[]} />);
    expect(getByTestId('aprs-positions-map')).toBeInTheDocument();
  });

  it('builds one feature per heard position with callsign + comment + coords', () => {
    render(<AprsPositionsMap positions={positions} />);
    const map = loadLast();
    const feats = sourceData(map, 'aprs-positions').features;
    expect(feats).toHaveLength(2);
    const xyz = feats.find((f) => f.properties.call === 'KK6XYZ')!;
    expect(xyz.properties.comment).toBe('Hello');
    // GeoJSON coordinate order is [lon, lat].
    expect(xyz.geometry.coordinates).toEqual([-72.03, 49.05]);
  });

  it('plots nothing when no positions are heard', () => {
    render(<AprsPositionsMap positions={[]} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features).toHaveLength(0);
  });

  // tuxlink-gq0d: the APRS map missed the tuxlink-vnk7 render-perf pattern that
  // StationFinderMap has. Lock the ported behavior so it can't regress.
  it('drives pin staleness via feature-state + a stable id, not a per-tick FC rebuild (gq0d)', () => {
    const now = Date.now();
    const fixes: HeardPosition[] = [
      { call: 'OLD', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: now - 20 * 60 * 1000, ambiguity: 0 },
      { call: 'NEW', lat: 41, lon: -101, symbolTable: '/', symbolCode: '>', comment: '', at: now, ambiguity: 0 },
    ];
    render(<AprsPositionsMap positions={fixes} />);
    const map = loadLast();
    const feats = sourceData(map, 'aprs-positions').features;
    const old = feats.find((f) => f.properties.call === 'OLD')! as PinFeature & { id?: string };
    // The pin carries a STABLE feature id (call) so feature-state can target it,
    // and staleness is NO LONGER a baked feature PROPERTY (which forced a full FC
    // rebuild every NOW_TICK) — it rides feature-state instead.
    expect(old.id).toBe('OLD');
    expect('stale' in old.properties).toBe(false);
    expect(map.__state.featureStates.get('aprs-positions')?.get('OLD')?.stale).toBe(true);
    expect(map.__state.featureStates.get('aprs-positions')?.get('NEW')?.stale).toBe(false);
  });

  it('re-pushes source data after a style swap (styledata) — two-effect usePushData (gq0d)', () => {
    render(<AprsPositionsMap positions={positions} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features).toHaveLength(2);
    // A flavor/pack change drops sources; the hooks re-add + re-push on styledata.
    act(() => map.setStyle({}));
    act(() => map.__emit('styledata'));
    expect(sourceData(map, 'aprs-positions').features).toHaveLength(2);
  });

  it('carries the ambiguity level onto each pin feature', () => {
    const amb: HeardPosition[] = [
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: 1, ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features[0].properties.ambiguity).toBe(2);
  });

  it('renders an uncertainty region only for ambiguous fixes (RF-honesty)', () => {
    const amb: HeardPosition[] = [
      { call: 'EXACT', lat: 49, lon: -72, symbolTable: '/', symbolCode: '-', comment: '', at: 1, ambiguity: 0 },
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: 2, ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    const circles = circleData(map, 'aprs-position-uncertainty').features;
    // Only the masked fix gets a region; the exact fix does not (no false halo).
    expect(circles).toHaveLength(1);
    expect(circles[0].properties.call).toBe('FUZZY');
    // It is a region (polygon), not a point — honest about the uncertainty.
    expect(circles[0].geometry.type).toBe('Polygon');
  });

  it('plots an ambiguous fix at the cell centre, not the decoded low corner', () => {
    const amb: HeardPosition[] = [
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    const [lon, lat] = sourceData(map, 'aprs-positions').features[0].geometry.coordinates;
    // Centre is shifted toward increasing magnitude on each axis (N => +lat,
    // W => more-negative lon), so it is NOT the raw decoded [-100, 40].
    expect(lat).toBeGreaterThan(40);
    expect(lon).toBeLessThan(-100);
  });

  it('plots an exact fix at its decoded coordinate (no centre shift)', () => {
    const exact: HeardPosition[] = [
      { call: 'EXACT', lat: 40, lon: -100, symbolTable: '/', symbolCode: '-', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    render(<AprsPositionsMap positions={exact} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features[0].geometry.coordinates).toEqual([-100, 40]);
  });

  it('maps ambiguity level to a growing uncertainty radius; 0 = none', () => {
    expect(ambiguityRadiusMeters(0)).toBe(0);
    expect(ambiguityRadiusMeters(1)).toBeGreaterThan(0);
    expect(ambiguityRadiusMeters(2)).toBeGreaterThan(ambiguityRadiusMeters(1));
    expect(ambiguityRadiusMeters(4)).toBeGreaterThan(ambiguityRadiusMeters(3));
  });

  it('popup discloses last-heard age and approximate-position note on click', () => {
    const fuzzy: HeardPosition[] = [
      {
        call: 'FUZZY',
        lat: 40,
        lon: -100,
        symbolTable: '/',
        symbolCode: '>',
        comment: 'mobile',
        // Heard ~20 min ago so the age is non-trivial and the pin is stale.
        at: Date.now() - 20 * 60 * 1000,
        ambiguity: 2,
      },
    ];
    const { getByTestId } = render(<AprsPositionsMap positions={fuzzy} />);
    const map = loadLast();
    act(() => map.__emit('click:aprs-position-pins-color', { features: [{ properties: { call: 'FUZZY' } }] }));
    expect(getByTestId('aprs-position-age').textContent).toContain('min ago');
    expect(getByTestId('aprs-position-ambiguity').textContent).toContain('approximate');
  });

  it('popup names the decoded APRS symbol on click', () => {
    const stations: HeardPosition[] = [
      // "/_" = primary table, code '_' → Weather station.
      { call: 'WX1AA', lat: 41, lon: -72, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0 },
      // "/>" = primary table, code '>' → Car.
      { call: 'MOBILE', lat: 42, lon: -71, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    const { getByTestId } = render(<AprsPositionsMap positions={stations} />);
    const map = loadLast();
    act(() => map.__emit('click:aprs-position-pins-color', { features: [{ properties: { call: 'WX1AA' } }] }));
    expect(getByTestId('aprs-position-symbol').textContent).toContain('Weather station');
    act(() => map.__emit('click:aprs-position-pins-color', { features: [{ properties: { call: 'MOBILE' } }] }));
    expect(getByTestId('aprs-position-symbol').textContent).toContain('Car');
  });

  // tuxlink-90xb: authentic symbol sprites on pins.
  it('builds features carrying stable colour + grey sprite ids', () => {
    const car: HeardPosition[] = [
      { call: 'W7RPT-9', lat: 45, lon: -73, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    render(<AprsPositionsMap positions={car} />);
    const map = loadLast();
    const props = sourceData(map, 'aprs-positions').features[0].properties as unknown as Record<string, unknown>;
    expect(props.spriteId).toBe('aprs:p:>');
    expect(props.spriteIdGrey).toBe('aprs:p:>:grey');
  });

  it('adds two icon layers that cross-fade colour->grey on the stale feature-state', () => {
    render(<AprsPositionsMap positions={positions} />);
    const map = loadLast();
    const color = map.__state.layers.find((l) => l.id === 'aprs-position-pins-color')!;
    const grey = map.__state.layers.find((l) => l.id === 'aprs-position-pins-grey')!;
    expect(color.spec.type).toBe('symbol');
    expect(grey.spec.type).toBe('symbol');
    expect((color.spec.layout as Record<string, unknown>)['icon-image']).toEqual(['get', 'spriteId']);
    expect((grey.spec.layout as Record<string, unknown>)['icon-image']).toEqual(['get', 'spriteIdGrey']);
    // icon-opacity (paint, so it can read feature-state) is the no-FC-churn channel.
    expect(JSON.stringify(color.spec.paint)).toContain('feature-state');
    expect(JSON.stringify(grey.spec.paint)).toContain('feature-state');
  });

  it('shrinks ambiguous pins via icon-size and keeps the uncertainty disc (f717)', () => {
    const amb: HeardPosition[] = [
      { call: 'N7CPZ', lat: 45, lon: -73, symbolTable: '/', symbolCode: '-', comment: '', at: Date.now(), ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    const color = map.__state.layers.find((l) => l.id === 'aprs-position-pins-color')!;
    expect(JSON.stringify((color.spec.layout as Record<string, unknown>)['icon-size'])).toContain('ambiguity');
    expect(map.__state.layers.some((l) => l.id === 'aprs-position-uncertainty-fill')).toBe(true);
  });

  it('registers a colour + grey image for each heard symbol', () => {
    const car: HeardPosition[] = [
      { call: 'W7RPT-9', lat: 45, lon: -73, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    render(<AprsPositionsMap positions={car} />);
    const map = loadLast();
    expect(map.hasImage('aprs:p:>')).toBe(true);
    expect(map.hasImage('aprs:p:>:grey')).toBe(true);
  });
});

describe('AprsPositionsMap WX overlay (ni5b)', () => {
  const wxPositions: HeardPosition[] = [
    { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: 1, ambiguity: 0 },
  ];
  const wxEnv = [
    {
      call: 'W7WX',
      project: '',
      seq: null,
      bits: [],
      rain: null,
      lastHeard: 1,
      channels: [
        { key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: 72, scaled: true, history: [] },
      ],
    },
  ];

  it('renders a WX badge layer + feature for a heard weather station', () => {
    render(<AprsPositionsMap positions={wxPositions} envStations={wxEnv as never} operatorGrid="CN87" />);
    const map = loadLast();
    expect(map.__state.layers.some((l) => l.id === 'aprs-wx-badge')).toBe(true);
    const feats = (map.getSource('aprs-wx-badge') as { data: { features: Array<{ properties: { badge: string } }> } }).data.features;
    expect(feats).toHaveLength(1);
    expect(feats[0].properties.badge).toContain('72°F');
  });

  it('invokes onFocusStation when a WX badge is clicked', () => {
    const onFocus = vi.fn();
    render(<AprsPositionsMap positions={wxPositions} envStations={wxEnv as never} onFocusStation={onFocus} operatorGrid="CN87" />);
    const map = loadLast();
    act(() => map.__emit('click:aprs-wx-badge', { features: [{ properties: { call: 'W7WX' } }] }));
    expect(onFocus).toHaveBeenCalledWith('W7WX');
  });

  it('renders no WX badge for a station with no heard weather', () => {
    render(<AprsPositionsMap positions={positions} envStations={[] as never} operatorGrid="CN87" />);
    const map = loadLast();
    const feats = (map.getSource('aprs-wx-badge') as { data: { features: unknown[] } }).data.features;
    expect(feats).toHaveLength(0);
  });
});

describe('AprsPositionsMap digipeat path (cn84)', () => {
  // KE7XYZ-9 heard via WIDE2-1 (repeated). WIDE2-1 has no heard position, so the
  // honest path is a single dashed src→you connector (the unlocatable-hop case).
  const viaPositions: HeardPosition[] = [
    {
      call: 'KE7XYZ-9',
      lat: 48.1,
      lon: -122.6,
      symbolTable: '/',
      symbolCode: '>',
      comment: '',
      at: 3,
      ambiguity: 0,
      via: [{ call: 'WIDE2-1', repeated: true }],
    },
  ];

  it('registers the path + packet-dot layers', () => {
    render(<AprsPositionsMap positions={viaPositions} operatorGrid="CN87" />);
    const map = loadLast();
    const ids = map.__state.layers.map((l) => l.id);
    expect(ids).toContain('aprs-digipeat-path-solid');
    expect(ids).toContain('aprs-digipeat-path-dashed');
    expect(ids).toContain('aprs-digipeat-packet-dot');
  });

  it('paints the honest path on pin hover and clears it on mouse-out', () => {
    render(<AprsPositionsMap positions={viaPositions} operatorGrid="CN87" />);
    const map = loadLast();
    act(() =>
      map.__emit('mouseenter:aprs-position-pins-color', {
        features: [{ properties: { call: 'KE7XYZ-9' } }],
      }),
    );
    const data = (map.getSource('aprs-digipeat-path') as { data: { features: unknown[] } }).data;
    expect(data.features.length).toBeGreaterThan(0);
    act(() => map.__emit('mouseleave:aprs-position-pins-color'));
    const cleared = (map.getSource('aprs-digipeat-path') as { data: { features: unknown[] } }).data;
    expect(cleared.features).toHaveLength(0);
  });

  it('renders a dashed segment for an unlocatable WIDE alias hop (RF-honesty)', () => {
    render(<AprsPositionsMap positions={viaPositions} operatorGrid="CN87" />);
    const map = loadLast();
    act(() =>
      map.__emit('mouseenter:aprs-position-pins-color', {
        features: [{ properties: { call: 'KE7XYZ-9' } }],
      }),
    );
    const data = (
      map.getSource('aprs-digipeat-path') as {
        data: { features: Array<{ properties: { kind: string } }> };
      }
    ).data;
    // WIDE2-1 has no heard position → no false-exact intermediate pin; the whole
    // src→you path is dashed, not solid.
    expect(data.features.length).toBeGreaterThan(0);
    expect(data.features.every((f) => f.properties.kind === 'dashed')).toBe(true);
  });

  it('marks the unlocatable hop with a pos? label', () => {
    render(<AprsPositionsMap positions={viaPositions} operatorGrid="CN87" />);
    const map = loadLast();
    act(() =>
      map.__emit('mouseenter:aprs-position-pins-color', {
        features: [{ properties: { call: 'KE7XYZ-9' } }],
      }),
    );
    const labels = (
      map.getSource('aprs-digipeat-path-labels') as {
        data: { features: Array<{ properties: { label: string } }> };
      }
    ).data;
    expect(labels.features.length).toBeGreaterThan(0);
    expect(labels.features[0].properties.label).toContain('WIDE2-1');
    expect(labels.features[0].properties.label).toContain('?');
  });

  it('does not trace a path from an object/item report (honest RF source)', () => {
    // An object pin plots the object's location with the SENDER's via-chain; the
    // path would fabricate the RF source, so it must not draw.
    const objPositions: HeardPosition[] = [
      {
        call: 'LEADER',
        lat: 48.1,
        lon: -122.6,
        symbolTable: '\\',
        symbolCode: '!',
        comment: '',
        at: 3,
        ambiguity: 0,
        isObject: true,
        via: [{ call: 'W7RPT-1', repeated: true }],
      },
    ];
    render(<AprsPositionsMap positions={objPositions} operatorGrid="CN87" />);
    const map = loadLast();
    act(() =>
      map.__emit('mouseenter:aprs-position-pins-color', {
        features: [{ properties: { call: 'LEADER' } }],
      }),
    );
    const data = (map.getSource('aprs-digipeat-path') as { data: { features: unknown[] } }).data;
    expect(data.features).toHaveLength(0);
  });
});


describe('AprsPositionsMap viewport persistence (tuxlink-dwzu)', () => {
  const KEY = 'tuxlink:map-viewport:aprs';
  beforeEach(() => window.localStorage.clear());

  it('opens at the saved viewport when one is stored', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ center: { lat: 45, lon: -73 }, zoom: 7 }));
    render(<AprsPositionsMap positions={[]} />);
    const map = getLastMap()!;
    expect(map.__state.options.center).toEqual([-73, 45]);
    expect(map.getZoom()).toBe(7);
  });

  it('centers on the operator at the local zoom on first run when an operator grid is known', () => {
    render(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />);
    const map = getLastMap()!;
    const me = gridToLatLon('DM43bp')!;
    expect(map.__state.options.center).toEqual([me.lon, me.lat]); // operator, not mid-Atlantic
    expect(map.getZoom()).toBe(10); // local area, not the continental Z6
  });

  it('falls back to the world view on first run only when no operator grid is known', () => {
    render(<AprsPositionsMap positions={[]} operatorGrid="" />);
    const map = getLastMap()!;
    expect(map.__state.options.center).toEqual([0, 0]);
    expect(map.getZoom()).toBe(2);
  });

  it('persists the viewport after a pan (moveend, debounced)', () => {
    vi.useFakeTimers();
    try {
      render(<AprsPositionsMap positions={[]} />);
      const map = getLastMap()!;
      act(() => map.__emit('load'));
      map.__setCenter(-122.3, 47.6);
      map.__setZoom(9);
      act(() => map.__emit('moveend'));
      act(() => vi.advanceTimersByTime(400));
      expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
        center: { lat: 47.6, lon: -122.3 },
        zoom: 9,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('recenters on the operator at OPERATOR_ZOOM when the recenter control is clicked', () => {
    render(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />);
    const map = getLastMap()!;
    act(() => map.__emit('load'));
    const me = gridToLatLon('DM43bp')!;
    fireEvent.click(screen.getByTestId('map-recenter'));
    expect(map.flyTo).toHaveBeenCalledWith({ center: [me.lon, me.lat], zoom: 10 });
  });

  it('renders an operator "you" pin at the operator grid (none when no grid is set)', () => {
    const { rerender } = render(<AprsPositionsMap positions={[]} operatorGrid="" />);
    const map = getLastMap()!;
    act(() => map.__emit('load'));
    expect(sourceData(map, 'aprs-operator').features).toHaveLength(0);
    act(() => rerender(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />));
    expect(sourceData(map, 'aprs-operator').features).toHaveLength(1);
  });

  it('hides the recenter control when no operator grid is known', () => {
    render(<AprsPositionsMap positions={[]} operatorGrid="" />);
    act(() => getLastMap()!.__emit('load'));
    expect(screen.queryByTestId('map-recenter')).toBeNull();
  });
});
