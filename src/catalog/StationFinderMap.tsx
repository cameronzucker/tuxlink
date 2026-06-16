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
const STATION_SEL_GLOW_LAYER = 'station-sel-glow';

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

// Pin radius (px) + colour per reachability tier — mirrors PIN_SIZE/2 and the
// --reach-* CSS vars. Data-driven so one circle layer paints every tier.
// Per-tier hex MUST mirror the --reach-* CSS vars (MapLibre paint can't read CSS
// custom properties). Six-step green→red→grey ramp; see reachability.ts.
const TIER_COLOR_MATCH = [
  'match',
  ['get', 'tier'],
  'good', '#41ba6c',
  'fair', '#8cc23f',
  'marginal', '#d9b13a',
  'poor', '#e2862f',
  'unlikely', '#d64a40', // red — almost certainly not
  'skip', '#6c5a5a', // grey — not reachable, inside radius
  '#9fb6cc', // untiered (no usable channel / no prediction)
];

const STATION_LAYERS = (
  [
    {
      // Soft selection GLOW — a filled (non-transparent) white disc drawn BENEATH
      // the pins, sized larger than the selected pin so a halo shows around it.
      // Filled (not a transparent-fill ring) so the GL path never culls it. Radius
      // 0 / opacity 0 when not selected → paints nothing. Selection is driven by
      // `feature-state` (B9, tuxlink-vnk7).
      id: STATION_SEL_GLOW_LAYER,
      type: 'circle',
      source: STATIONS_SOURCE,
      paint: {
        'circle-radius': ['case', ['boolean', ['feature-state', 'selected'], false], 12, 0],
        'circle-color': '#ffffff',
        'circle-opacity': ['case', ['boolean', ['feature-state', 'selected'], false], 0.25, 0],
        'circle-blur': 0.6,
      },
    },
    {
      id: STATION_PINS_LAYER,
      type: 'circle',
      source: STATIONS_SOURCE,
      paint: {
        // Selected pin gets a MODEST bump + a bright-white rim (a soft glow sits
        // beneath) — enough to read clearly without ballooning (operator
        // 2026-06-16: the prior +5/glow-18 looked huge). Selection is
        // `feature-state`-driven so a click flips one feature's state, not the FC.
        'circle-radius': [
          'case',
          ['boolean', ['feature-state', 'selected'], false],
          ['match', ['get', 'tier'], 'good', 12, 'fair', 10, 'marginal', 8.5, 'poor', 7.5, 'unlikely', 7, 'skip', 6.5, 9],
          ['match', ['get', 'tier'], 'good', 10, 'fair', 8, 'marginal', 6.5, 'poor', 5.5, 'unlikely', 5, 'skip', 4.5, 7],
        ],
        'circle-color': TIER_COLOR_MATCH,
        // The grey "not reachable" bottom tier sits back (dimmer) so the live
        // red/orange/green stations read first; red "unlikely" stays full.
        'circle-opacity': ['case', ['==', ['get', 'tier'], 'skip'], 0.7, 1],
        // Selected → bright-white rim; others keep a thin white rim for basemap
        // contrast. White renders reliably on this webview.
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': ['case', ['boolean', ['feature-state', 'selected'], false], 2, 0.6],
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

function buildStationFC(stations: Station[], tiers: Map<string, ReachTier>): FeatureCollection {
  const features: unknown[] = [];
  for (const s of stations) {
    const ll = gridToLatLon(s.grid);
    if (!ll) continue;
    const key = stationKey(s);
    features.push({
      // Top-level `id` (the station key) is what `setFeatureState` targets — a
      // string id is valid GeoJSON and lets selection flip one feature's state
      // without rebuilding the FeatureCollection (B9, tuxlink-vnk7).
      type: 'Feature',
      id: key,
      properties: { key, tier: tiers.get(key) ?? 'untiered' },
      geometry: { type: 'Point', coordinates: [ll.lon, ll.lat] },
    });
  }
  return { type: 'FeatureCollection', features };
}

/**
 * Pushes GeoJSON to a source on change, re-pushing on styledata (style swap).
 *
 * Subscribes `styledata` ONCE (deps `[map, sourceId]`) and re-pushes the latest
 * data from a ref; the push-on-change lives in a SEPARATE effect (deps
 * `[map, sourceId, data]`). The old single-effect form re-subscribed `styledata`
 * and full-replaced the source on EVERY data change (B9, tuxlink-vnk7); mirrors
 * the pattern in LocationMap.
 */
function usePushData(
  map: ReturnType<typeof useMapContext>,
  sourceId: string,
  data: FeatureCollection,
) {
  const dataRef = useRef(data);
  dataRef.current = data;

  // Subscribe `styledata` once; re-push the latest data after a style swap.
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(sourceId) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(dataRef.current);
    };
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map, sourceId]);

  // Push imperatively whenever the data actually changes.
  useEffect(() => {
    if (!map) return;
    const src = map.getSource(sourceId) as { setData?: (d: unknown) => void } | undefined;
    src?.setData?.(data);
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

  // Selection is feature-state-driven (below), so the FC rebuilds only when the
  // station set or tiers change — NOT on every selection click (B9, tuxlink-vnk7).
  const fc = useMemo(() => buildStationFC(stations, tiers), [stations, tiers]);

  // `promoteId` is REQUIRED for feature-state here: MapLibre only honors
  // feature-state on a GeoJSON source whose feature ids are numeric OR promoted
  // from a property — top-level STRING ids (our `CALL|GRID` station keys) are
  // silently ignored, so setFeatureState was a no-op and the selected pin never
  // got its emphasis (operator 2026-06-16, root cause). Promote the `key`
  // property (= the station key) to the feature id so selection actually paints.
  useMapOverlay(map, STATIONS_SOURCE, { type: 'geojson', data: EMPTY_FC, promoteId: 'key' }, STATION_LAYERS);
  usePushData(map, STATIONS_SOURCE, fc);

  // Drive selection via `setFeatureState` instead of rebuilding the FC: clear the
  // previously-selected feature's state and set `{selected:true}` on the new one.
  // Also RE-APPLY on `styledata` (Codex P2): a setStyle (flavor/pack change) drops
  // all feature-state, and the hooks re-add the source on the same tick — without
  // this the selected pin would lose its emphasis until selectedKey changes.
  const prevSelectedRef = useRef<string | null>(null);
  const selectedKeyRef = useRef<string | null>(selectedKey);
  selectedKeyRef.current = selectedKey;
  useEffect(() => {
    if (!map) return;
    const m = map as unknown as {
      setFeatureState?: (t: { source: string; id: string | number }, s: Record<string, unknown>) => void;
      removeFeatureState?: (t: { source: string; id?: string | number }, key?: string) => void;
      triggerRepaint?: () => void;
      on: (t: string, h: (...a: unknown[]) => void) => unknown;
      off: (t: string, h: (...a: unknown[]) => void) => unknown;
    };
    const apply = () => {
      const cur = selectedKeyRef.current;
      const prev = prevSelectedRef.current;
      if (prev != null && prev !== cur) {
        m.removeFeatureState?.({ source: STATIONS_SOURCE, id: prev }, 'selected');
      }
      if (cur != null) {
        m.setFeatureState?.({ source: STATIONS_SOURCE, id: cur }, { selected: true });
      }
      prevSelectedRef.current = cur;
      // Force a frame: a feature-state change does not always schedule a repaint
      // on its own, so the new emphasis wouldn't draw until the next map
      // interaction — the "needs two clicks" bug (operator 2026-06-16).
      m.triggerRepaint?.();
    };
    apply();
    // Re-apply after a style swap re-creates the source (clears feature-state).
    m.on('styledata', apply as (...a: unknown[]) => void);
    // Re-apply after a data push. `GeoJSONSource.setData` drops ALL feature-state
    // for the source, and reachability tiers stream in (one setData per update,
    // plus a full re-push on every prefs change), so without this the selected
    // pin loses its glow/emphasis the instant a tier updates — the "selection
    // doesn't show" bug (operator, 2026-06-16). `sourcedata` fires when the
    // source finishes (re)loading; re-applying then survives the async clear.
    const onSourceData = (e: { sourceId?: string; isSourceLoaded?: boolean }) => {
      if (e.sourceId === STATIONS_SOURCE && e.isSourceLoaded) apply();
    };
    m.on('sourcedata', onSourceData as (...a: unknown[]) => void);
    return () => {
      m.off('styledata', apply as (...a: unknown[]) => void);
      m.off('sourcedata', onSourceData as (...a: unknown[]) => void);
    };
  }, [map, selectedKey]);

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
        <span className="k poor" /> maybe not
        <span className="k unlikely" /> unlikely
        <span className="k skip" /> not reachable
      </div>
    </div>
  );
}
