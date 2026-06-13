// Left-pane station map (design §7). One pin per station at its grid centroid,
// coloured/sized by its reachability tier on the selected band; an operator
// "you" pin; click-to-select.
//
// MapLibre re-expression (tuxlink-ndi4 phase 2): pins are GeoJSON CIRCLE layers
// with data-driven radius/colour/selected-stroke — NOT maplibregl.Marker. Circle
// layers are CSP-safe and need no per-pin DOM, sidestepping both the historical
// divIcon CSP "black blob" (s0r1) and the A13 packaged-marker risk. Click-select
// is a layer-scoped `map.on('click', 'station-pins', …)`. Render fidelity is
// grim-verified; the unit test proves the source/layer/feature wiring (C1).
import { useEffect, useMemo, useRef } from 'react';
import { MapLibreMap } from '../map/MapLibreMap';
import { useMapContext } from '../map/MapContext';
import { useMapOverlay } from '../map/mapHooks';
import { gridToLatLon } from '../forms/position/maidenhead';
import { type ReachTier } from './reachability';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

export interface StationFinderMapProps {
  stations: Station[];
  operatorGrid: string;
  tiers: Map<string, ReachTier>;
  selectedKey: string | null;
  onSelect: (station: Station) => void;
}

// Recenter zoom on the operator, on the z0–14 scale (was raster-native z3; finding 2).
const OPERATOR_ZOOM = 6;

const STATIONS_SOURCE = 'stations';
const OPERATOR_SOURCE = 'operator';
const STATION_PINS_LAYER = 'station-pins';

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

// Pin radius (px) + colour per reachability tier — mirrors PIN_SIZE/2 and the
// --reach-* CSS vars. Data-driven so one circle layer paints every tier.
const STATION_LAYERS = (
  [
    {
      id: STATION_PINS_LAYER,
      type: 'circle',
      source: STATIONS_SOURCE,
      paint: {
        'circle-radius': ['match', ['get', 'tier'], 'good', 10, 'fair', 8, 'marginal', 6.5, 'skip', 5, 7],
        'circle-color': [
          'match',
          ['get', 'tier'],
          'good', '#46d07f',
          'fair', '#c9b23a',
          'marginal', '#d2842f',
          'skip', '#6c5a5a',
          '#9fb6cc',
        ],
        'circle-opacity': ['case', ['==', ['get', 'tier'], 'skip'], 0.75, 1],
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': ['case', ['get', 'selected'], 2, 0.5],
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

const OPERATOR_LAYERS = (
  [
    {
      id: 'operator-pin',
      type: 'circle',
      source: OPERATOR_SOURCE,
      paint: {
        'circle-radius': 7,
        'circle-color': '#eaf3fb',
        'circle-stroke-color': '#2f86f0',
        'circle-stroke-width': 3,
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

function buildStationFC(
  stations: Station[],
  tiers: Map<string, ReachTier>,
  selectedKey: string | null,
): FeatureCollection {
  const features: unknown[] = [];
  for (const s of stations) {
    const ll = gridToLatLon(s.grid);
    if (!ll) continue;
    const key = stationKey(s);
    features.push({
      type: 'Feature',
      properties: { key, tier: tiers.get(key) ?? 'untiered', selected: selectedKey === key },
      geometry: { type: 'Point', coordinates: [ll.lon, ll.lat] },
    });
  }
  return { type: 'FeatureCollection', features };
}

/** Pushes GeoJSON to a source on change, re-pushing on styledata (style swap). */
function usePushData(
  map: ReturnType<typeof useMapContext>,
  sourceId: string,
  data: FeatureCollection,
) {
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(sourceId) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(data);
    };
    push();
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map, sourceId, data]);
}

function StationLayers({ stations, tiers, selectedKey, onSelect }: Omit<StationFinderMapProps, 'operatorGrid'>) {
  const map = useMapContext();

  const byKey = useMemo(() => {
    const m = new Map<string, Station>();
    for (const s of stations) m.set(stationKey(s), s);
    return m;
  }, [stations]);
  const byKeyRef = useRef(byKey);
  byKeyRef.current = byKey;
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  const fc = useMemo(() => buildStationFC(stations, tiers, selectedKey), [stations, tiers, selectedKey]);

  useMapOverlay(map, STATIONS_SOURCE, { type: 'geojson', data: EMPTY_FC }, STATION_LAYERS);
  usePushData(map, STATIONS_SOURCE, fc);

  useEffect(() => {
    if (!map) return;
    const onClick = (e: { features?: Array<{ properties?: { key?: unknown } }> }) => {
      const key = e.features?.[0]?.properties?.key;
      if (key == null) return;
      const station = byKeyRef.current.get(String(key));
      if (station) onSelectRef.current(station);
    };
    map.on('click', STATION_PINS_LAYER, onClick as (...a: unknown[]) => void);
    return () => {
      map.off('click', STATION_PINS_LAYER, onClick as (...a: unknown[]) => void);
    };
  }, [map]);

  return null;
}

function OperatorPin({ location }: { location: { lat: number; lon: number } | null }) {
  const map = useMapContext();
  const fc = useMemo<FeatureCollection>(
    () =>
      location
        ? {
            type: 'FeatureCollection',
            features: [
              { type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [location.lon, location.lat] } },
            ],
          }
        : EMPTY_FC,
    [location?.lat, location?.lon],
  );
  useMapOverlay(map, OPERATOR_SOURCE, { type: 'geojson', data: EMPTY_FC }, OPERATOR_LAYERS);
  usePushData(map, OPERATOR_SOURCE, fc);
  return null;
}

export function StationFinderMap(props: StationFinderMapProps) {
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  return (
    <div className="station-finder__map" data-testid="station-map">
      <MapLibreMap initialCenter={me ?? undefined} initialZoom={me ? OPERATOR_ZOOM : 2}>
        <StationLayers
          stations={props.stations}
          tiers={props.tiers}
          selectedKey={props.selectedKey}
          onSelect={props.onSelect}
        />
        <OperatorPin location={me} />
      </MapLibreMap>
      <div className="station-finder__reachkey" aria-hidden>
        <span className="k good" /> good
        <span className="k fair" /> fair
        <span className="k marginal" /> marginal
        <span className="k skip" /> unlikely
      </div>
    </div>
  );
}
