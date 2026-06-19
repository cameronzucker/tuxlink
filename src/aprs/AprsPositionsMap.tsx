// APRS Tac Chat positions map (tuxlink-6vgt). Plots the positions of stations
// HEARD on the open channel — one pin per station at its decoded lat/lon, with a
// callsign label and a comment popup. RF-honesty: every pin is a real, decoded
// fix (no estimated locations); a station appears only after its beacon is heard.
//
// RF-honesty refinements (tuxlink-f717):
//   - Ambiguous fixes (APRS position-ambiguity > 0, where the sender masked
//     low-order minute digits) are drawn as an UNCERTAINTY REGION — a translucent
//     circle sized to the masked resolution — instead of a false-exact pin, so the
//     map never claims more precision than the wire carried.
//   - Pins age: a station not re-heard within STALE_MS dims, and the popup shows
//     how long ago it was last heard, so a stale fix is not read as current.
//
// Reuses the MapLibre stack (MapLibreMap + MapContext + the owned useMapOverlay
// hook) directly rather than StationFinderMap, which is hardwired to catalog
// Station[] + reachability tiers + Maidenhead-grid centroids. Pins are GeoJSON
// CIRCLE + SYMBOL (text) layers; uncertainty regions are GeoJSON FILL + LINE
// layers — CSP-safe, no per-pin DOM — mirroring the circle-layer pattern
// StationFinderMap established (tuxlink-ndi4).

import { useEffect, useMemo, useRef, useState } from 'react';
import { MapLibreMap } from '../map/MapLibreMap';
import { useMapContext } from '../map/MapContext';
import { useMapOverlay } from '../map/mapHooks';
import { usePersistedViewport } from '../map/usePersistedViewport';
import { RecenterControl } from '../map/RecenterControl';
import { gridToLatLon } from '../forms/position/maidenhead';
import { lookupAprsSymbol } from './aprsSymbols';
import {
  spriteIdFor,
  greyIdOf,
  ensureSymbolImage,
  whenSheetsReady,
  type SpriteMap,
} from '../map/aprsSprites';
import type { HeardPosition } from './aprsTypes';
import { resolveDigipeatPath, type LatLon, type PathSegment } from './digipeatPath';
import { computeTraceFrame, DEFAULT_TIMINGS, type ActiveTrace } from './pathTrace';
import './AprsPositionsMap.css';

export interface AprsPositionsMapProps {
  /// Heard stations' latest positions (one per callsign), from useAprsPositions.
  positions: HeardPosition[];
  /// Operator Maidenhead grid (statusData.ui_grid). First-run map center; the
  /// recenter control flies here. Empty / absent = no known position.
  operatorGrid?: string;
}

/// First-run / recenter zoom on the operator. APRS is local VHF, so this is a
/// LOCAL-area zoom (metro/county) — not StationFinderMap's continental Z6, which
/// suits its national HF-gateway context. Tunable; operator feedback drove this.
const OPERATOR_ZOOM = 10;

const POSITIONS_SOURCE = 'aprs-positions';
// tuxlink-90xb: pins are now authentic symbol SPRITES on two stacked icon layers
// (colour + greyscale) that cross-fade on the `stale` feature-state, replacing the
// single circle layer. Identity (the sprite) + honesty (stale/ambiguous) coexist.
const POSITION_PINS_COLOR_LAYER = 'aprs-position-pins-color';
const POSITION_PINS_GREY_LAYER = 'aprs-position-pins-grey';
const POSITION_LABELS_LAYER = 'aprs-position-labels';
const UNCERTAINTY_SOURCE = 'aprs-position-uncertainty';
const UNCERTAINTY_FILL_LAYER = 'aprs-position-uncertainty-fill';
const UNCERTAINTY_LINE_LAYER = 'aprs-position-uncertainty-outline';
// The operator's OWN position ("you" pin). Not a decoded beacon — it's the
// known operator grid, drawn distinctly (blue-ringed) so it reads as "me", not a
// heard station. Does not violate the map's RF-honesty (it is not claimed to be
// a received fix).
const OPERATOR_SOURCE = 'aprs-operator';
const OPERATOR_PIN_LAYER = 'aprs-operator-pin';

// cn84: the animated digipeat path. One line source (solid green for located
// hops, dashed amber across unknown ones — see resolveDigipeatPath) plus a
// one-point source for the riding "packet" dot.
const PATH_SOURCE = 'aprs-digipeat-path';
const PATH_SOLID_LAYER = 'aprs-digipeat-path-solid';
const PATH_DASHED_LAYER = 'aprs-digipeat-path-dashed';
const PATH_DOT_SOURCE = 'aprs-digipeat-packet';
const PATH_DOT_LAYER = 'aprs-digipeat-packet-dot';
// `pos?` markers: the honest cue for a hop we can't locate (WIDE aliases, unheard
// digis). One amber text label at the midpoint of each dashed connector.
const PATH_LABEL_SOURCE = 'aprs-digipeat-path-labels';
const PATH_LABEL_LAYER = 'aprs-digipeat-path-label';

const PATH_LAYERS = (
  [
    {
      id: PATH_SOLID_LAYER,
      type: 'line',
      source: PATH_SOURCE,
      filter: ['==', ['get', 'kind'], 'solid'],
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: {
        'line-color': '#7fe6a3',
        'line-width': 2.5,
        'line-opacity': ['coalesce', ['get', 'opacity'], 1],
      },
    },
    {
      id: PATH_DASHED_LAYER,
      type: 'line',
      source: PATH_SOURCE,
      filter: ['==', ['get', 'kind'], 'dashed'],
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: {
        'line-color': '#f0c987',
        'line-width': 2,
        'line-dasharray': [1.5, 1.5],
        'line-opacity': ['coalesce', ['get', 'opacity'], 1],
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

const PATH_DOT_LAYERS = (
  [
    {
      id: PATH_DOT_LAYER,
      type: 'circle',
      source: PATH_DOT_SOURCE,
      paint: {
        'circle-radius': 4,
        'circle-color': '#ffffff',
        'circle-stroke-color': '#0b1218',
        'circle-stroke-width': 1,
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

const PATH_LABEL_LAYERS = (
  [
    {
      id: PATH_LABEL_LAYER,
      type: 'symbol',
      source: PATH_LABEL_SOURCE,
      layout: {
        'text-field': ['get', 'label'],
        'text-size': 10,
        'text-offset': [0, -0.8],
        'text-anchor': 'bottom',
        'text-allow-overlap': true,
      },
      paint: {
        'text-color': '#f0c987',
        'text-halo-color': '#0c1620',
        'text-halo-width': 1.2,
        'text-opacity': ['coalesce', ['get', 'opacity'], 1],
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

/// A fix not re-heard within this long is shown dimmed (and its age is surfaced
/// in the popup). The hook drops it entirely after a longer TTL.
const STALE_MS = 15 * 60 * 1000;
/// Cadence for recomputing "now" so staleness updates without new traffic.
const NOW_TICK_MS = 30 * 1000;

/// Uncertainty radius (in latitude minutes) for each APRS ambiguity level. Level
/// N masks the lowest N minute digits, so the fix could lie anywhere in a box
/// half this many minutes wide: L1 ±0.05′, L2 ±0.5′, L3 ±5′, L4 ±30′ (1°).
const AMBIGUITY_HALF_MINUTES = [0, 0.05, 0.5, 5, 30];
const METERS_PER_MINUTE_LAT = 1852;

/// Half-width, in metres, of the ambiguity cell for a given level — the "±"
/// distance shown in the popup. `0` for a full-precision fix (level 0).
export function ambiguityRadiusMeters(level: number): number {
  const l = Math.max(0, Math.min(4, Math.floor(level)));
  return AMBIGUITY_HALF_MINUTES[l] * METERS_PER_MINUTE_LAT;
}

/// The decoded coordinate is the LOW corner of the ambiguity cell (the parser
/// zero-fills masked minute digits), so plot the cell CENTRE — half a cell
/// toward increasing magnitude on each axis — and let the region circumscribe
/// the box. A full-precision fix is returned unchanged.
function cellCenter(p: HeardPosition): { lon: number; lat: number } {
  const l = Math.max(0, Math.min(4, Math.floor(p.ambiguity)));
  const offDeg = AMBIGUITY_HALF_MINUTES[l] / 60;
  if (offDeg === 0) return { lon: p.lon, lat: p.lat };
  return {
    lat: p.lat + Math.sign(p.lat) * offDeg,
    lon: p.lon + Math.sign(p.lon) * offDeg,
  };
}

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

// tuxlink-90xb: two stacked symbol layers draw the authentic sprite. The GREY
// layer sits under the COLOUR layer; their `icon-opacity` (a PAINT prop, so it
// can read feature-state) cross-fades colour->greyscale when `stale` flips —
// keeping staleness a feature-state toggle with NO FeatureCollection rebuild
// (tuxlink-gq0d). Ambiguous fixes shrink via the data-driven icon-size; the amber
// uncertainty disc beneath them (tuxlink-f717) is unchanged. A third symbol layer
// draws the callsign label above the pin.
const ICON_LAYOUT: Record<string, unknown> = {
  'icon-allow-overlap': true,
  'icon-ignore-placement': true,
  // 32px display from 64px cells at pixelRatio 2 => icon-size 1; ambiguous shrink.
  'icon-size': ['case', ['>', ['get', 'ambiguity'], 0], 0.7, 1],
  'icon-anchor': 'center',
};
const POSITION_LAYERS = (
  [
    {
      id: POSITION_PINS_GREY_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: { ...ICON_LAYOUT, 'icon-image': ['get', 'spriteIdGrey'] },
      paint: {
        'icon-opacity': ['case', ['boolean', ['feature-state', 'stale'], false], 0.55, 0],
      },
    },
    {
      id: POSITION_PINS_COLOR_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: { ...ICON_LAYOUT, 'icon-image': ['get', 'spriteId'] },
      paint: {
        'icon-opacity': ['case', ['boolean', ['feature-state', 'stale'], false], 0, 0.95],
      },
    },
    {
      id: POSITION_LABELS_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: {
        'text-field': ['get', 'call'],
        'text-size': 11,
        'text-offset': [0, -1.4],
        'text-anchor': 'bottom',
      },
      paint: {
        'text-color': '#eaf3fb',
        'text-halo-color': '#0c1620',
        'text-halo-width': 1.2,
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

// Translucent amber disc + dashed outline beneath the pins, one per ambiguous
// station — the honest depiction of "somewhere in this region", not a point.
const UNCERTAINTY_LAYERS = (
  [
    {
      id: UNCERTAINTY_FILL_LAYER,
      type: 'fill',
      source: UNCERTAINTY_SOURCE,
      paint: { 'fill-color': '#f0c24a', 'fill-opacity': 0.12 },
    },
    {
      id: UNCERTAINTY_LINE_LAYER,
      type: 'line',
      source: UNCERTAINTY_SOURCE,
      paint: {
        'line-color': '#f0c24a',
        'line-opacity': 0.5,
        'line-width': 1,
        'line-dasharray': [2, 2],
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

// The operator's own "you" pin — a blue-ringed dot (mirrors StationFinderMap's
// operator pin) so it reads as "me", distinct from the blue heard-station pins.
const OPERATOR_LAYERS = (
  [
    {
      id: OPERATOR_PIN_LAYER,
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

/// A ring of [lon,lat] points approximating a circle of `radiusM` metres around
/// the centre. Closed (first point repeated) so it forms a valid GeoJSON polygon.
function circlePolygon(lon: number, lat: number, radiusM: number, steps = 48): number[][] {
  const dLat = radiusM / 111320;
  // Clamp cos(lat) so a near-polar fix (cos → 0) cannot blow dLon up to NaN /
  // out-of-range coordinates pushed into MapLibre.
  const cosLat = Math.max(Math.cos((lat * Math.PI) / 180), 0.01);
  const dLon = radiusM / (111320 * cosLat);
  const ring: number[][] = [];
  for (let i = 0; i <= steps; i++) {
    const theta = (i / steps) * 2 * Math.PI;
    ring.push([lon + dLon * Math.cos(theta), lat + dLat * Math.sin(theta)]);
  }
  return ring;
}

// tuxlink-gq0d: the FC depends ONLY on `positions` now — `stale` moved to
// feature-state (set on the NOW_TICK without rebuilding/re-pushing the whole FC).
// Each feature carries a stable top-level `id` (the callsign) so feature-state
// can target it, mirroring StationFinderMap's stationKey id.
function buildPositionFC(positions: HeardPosition[]): FeatureCollection {
  const features: unknown[] = positions.map((p) => {
    // Ambiguous fixes plot at the cell CENTRE as a deliberately soft, small
    // marker — never a sharp pin claiming a coordinate the packet did not carry.
    const c = cellCenter(p);
    // Stable per-station sprite ids (overlay folded in) — they never change as a
    // station goes stale, so the FC is not rebuilt on the staleness tick (gq0d).
    const spriteId = spriteIdFor(
      p.symbolTable,
      p.symbolCode,
      lookupAprsSymbol(p.symbolTable, p.symbolCode).overlay,
    );
    return {
      type: 'Feature',
      id: p.call,
      properties: {
        call: p.call,
        comment: p.comment,
        ambiguity: p.ambiguity,
        spriteId,
        spriteIdGrey: greyIdOf(spriteId),
      },
      geometry: { type: 'Point', coordinates: [c.lon, c.lat] },
    };
  });
  return { type: 'FeatureCollection', features };
}

/// Uncertainty regions for ambiguous fixes only — a full-precision fix gets no
/// halo, so the map never implies uncertainty the wire did not report. The
/// circle is centred on the ambiguity cell and sized (×√2) to circumscribe the
/// box, so it covers every coordinate the fix could actually be — never less.
function buildUncertaintyFC(positions: HeardPosition[]): FeatureCollection {
  const features: unknown[] = positions
    .filter((p) => p.ambiguity > 0)
    .map((p) => {
      const c = cellCenter(p);
      const r = ambiguityRadiusMeters(p.ambiguity) * Math.SQRT2;
      return {
        type: 'Feature',
        properties: { call: p.call, ambiguity: p.ambiguity },
        geometry: { type: 'Polygon', coordinates: [circlePolygon(c.lon, c.lat, r)] },
      };
    });
  return { type: 'FeatureCollection', features };
}

/// Human "last heard" age, e.g. "just now", "3 min ago", "2 h ago".
function formatAge(ms: number): string {
  if (ms < 60_000) return 'just now';
  const min = Math.floor(ms / 60_000);
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  return `${h} h ago`;
}

/// "± ~Xkm" / "± ~Xm" precision note for an ambiguous fix's popup.
function ambiguityNote(level: number): string {
  const r = ambiguityRadiusMeters(level);
  const approx = r >= 1000 ? `~${(r / 1000).toFixed(r >= 10000 ? 0 : 1)} km` : `~${Math.round(r)} m`;
  return `approximate position (±${approx})`;
}

/**
 * Pushes GeoJSON to a source on change, re-pushing on styledata (style swap).
 *
 * tuxlink-gq0d / tuxlink-vnk7 (B9): subscribe `styledata` ONCE (deps
 * `[map, sourceId]`, re-pushing the latest from a ref) and push-on-change in a
 * SEPARATE effect. The old single-effect form re-subscribed `styledata` AND
 * full-replaced the source on EVERY data change — the perf anti-pattern that made
 * this map churn vs StationFinderMap (which already uses this two-effect form).
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

function PositionLayers({ positions }: AprsPositionsMapProps) {
  const map = useMapContext();
  // Store only the SELECTED callsign; the popup body is derived from the live
  // `byCall` entry each render, so a re-beacon updates the open popup and a
  // pruned station closes it (no stale snapshot).
  const [popupCall, setPopupCall] = useState<string | null>(null);

  // Re-tick "now" so pins age (dim) and the popup age stays roughly current
  // even when no new traffic arrives.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), NOW_TICK_MS);
    return () => clearInterval(id);
  }, []);

  const byCall = useMemo(() => {
    const m = new Map<string, HeardPosition>();
    for (const p of positions) m.set(p.call, p);
    return m;
  }, [positions]);
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;

  // tuxlink-gq0d: the FC no longer depends on `now` — it rebuilds only when the
  // positions actually change, not every NOW_TICK. Staleness rides feature-state.
  const fc = useMemo(() => buildPositionFC(positions), [positions]);
  const uncertaintyFc = useMemo(() => buildUncertaintyFC(positions), [positions]);

  // Uncertainty regions register first so the pins + labels draw on top of them.
  useMapOverlay(map, UNCERTAINTY_SOURCE, { type: 'geojson', data: EMPTY_FC }, UNCERTAINTY_LAYERS);
  useMapOverlay(map, POSITIONS_SOURCE, { type: 'geojson', data: EMPTY_FC }, POSITION_LAYERS);
  // tuxlink-90xb: register the colour + grey image for every heard symbol before
  // the source data references it (a symbol layer silently skips an unregistered
  // icon-image). Re-applied on styledata — a style swap clears registered images.
  useEffect(() => {
    if (!map) return;
    const m = map as unknown as SpriteMap & {
      on: (t: string, h: (...a: unknown[]) => void) => unknown;
      off: (t: string, h: (...a: unknown[]) => void) => unknown;
    };
    const apply = (force = false) => {
      for (const p of positions) {
        ensureSymbolImage(
          m,
          p.symbolTable,
          p.symbolCode,
          lookupAprsSymbol(p.symbolTable, p.symbolCode).overlay,
          force,
        );
      }
    };
    apply();
    // tuxlink-r8sm: the sprite sheets decode asynchronously, but this first bake
    // runs synchronously on mount (positions are already accumulated by the time
    // the map opens) — before the PNGs decode, so those sprites bake transparent.
    // Re-bake (force) once the sheets are ready so pins actually show their icon.
    const stopWhenReady = whenSheetsReady(() => apply(true));
    const onStyle = () => apply();
    m.on('styledata', onStyle as (...a: unknown[]) => void);
    return () => {
      stopWhenReady();
      m.off('styledata', onStyle as (...a: unknown[]) => void);
    };
  }, [map, positions]);

  usePushData(map, UNCERTAINTY_SOURCE, uncertaintyFc);
  usePushData(map, POSITIONS_SOURCE, fc);

  // tuxlink-gq0d: drive pin staleness via feature-state (mirrors StationFinderMap's
  // selection) instead of rebuilding + re-pushing the whole FeatureCollection on
  // every NOW_TICK. Cheap: one setFeatureState per heard station. Re-applied on
  // styledata because a style swap (flavor/pack change) clears feature-state.
  useEffect(() => {
    if (!map) return;
    const m = map as unknown as {
      setFeatureState?: (t: { source: string; id: string | number }, s: Record<string, unknown>) => void;
      on: (t: string, h: (...a: unknown[]) => void) => unknown;
      off: (t: string, h: (...a: unknown[]) => void) => unknown;
    };
    const apply = () => {
      for (const p of positions) {
        m.setFeatureState?.({ source: POSITIONS_SOURCE, id: p.call }, { stale: now - p.at > STALE_MS });
      }
    };
    apply();
    m.on('styledata', apply as (...a: unknown[]) => void);
    return () => {
      m.off('styledata', apply as (...a: unknown[]) => void);
    };
  }, [map, positions, now]);

  // Click a pin → show its callsign + comment + last-heard age in an inline popup.
  useEffect(() => {
    if (!map) return;
    const onClick = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call == null) return;
      if (byCallRef.current.has(String(call))) setPopupCall(String(call));
    };
    map.on('click', POSITION_PINS_COLOR_LAYER, onClick as (...a: unknown[]) => void);
    return () => {
      map.off('click', POSITION_PINS_COLOR_LAYER, onClick as (...a: unknown[]) => void);
    };
  }, [map]);

  // Derive the popup body from the CURRENT fix for the selected call; if that
  // station was pruned (stale TTL), the popup closes on its own.
  const selected = popupCall ? byCall.get(popupCall) : undefined;
  if (!selected) return null;
  // Identify the station's APRS symbol from the table+code it transmitted, so
  // the popup names what kind of station this is (car, weather, digipeater, …).
  // RF-honesty: this only reflects the symbol actually on the wire.
  const symbol = lookupAprsSymbol(selected.symbolTable, selected.symbolCode);
  return (
    <div className="aprs-positions-map__popup" role="status" data-testid="aprs-position-popup">
      <button
        type="button"
        className="aprs-positions-map__popup-close"
        aria-label="Dismiss"
        onClick={() => setPopupCall(null)}
      >
        ×
      </button>
      <span className="aprs-positions-map__popup-call">{selected.call}</span>
      <span className="aprs-positions-map__popup-symbol" data-testid="aprs-position-symbol">
        <span className="aprs-positions-map__popup-symbol-glyph" aria-hidden="true">
          {symbol.glyph}
        </span>
        {symbol.overlay ? `${symbol.name} (overlay ${symbol.overlay})` : symbol.name}
      </span>
      <span className="aprs-positions-map__popup-age" data-testid="aprs-position-age">
        last heard {formatAge(Math.max(0, now - selected.at))}
      </span>
      {selected.ambiguity > 0 && (
        <span className="aprs-positions-map__popup-ambiguity" data-testid="aprs-position-ambiguity">
          {ambiguityNote(selected.ambiguity)}
        </span>
      )}
      {selected.comment && (
        <span className="aprs-positions-map__popup-comment">{selected.comment}</span>
      )}
    </div>
  );
}

/// One LineString feature per resolved segment, carrying its kind + the path
/// opacity. `progress` (0..1) trims the polyline to the drawn fraction so the
/// live trace draws in hop-by-hop; at progress 1 every segment is full.
function pathFC(segments: PathSegment[], progress: number, opacity: number): FeatureCollection {
  const total = segments.length;
  const features: unknown[] = [];
  segments.forEach((s, i) => {
    const segStart = i / total;
    const segEnd = (i + 1) / total;
    if (progress <= segStart) return; // this segment not reached yet
    let to = s.to;
    if (progress < segEnd) {
      // Partially-drawn final segment: interpolate the visible endpoint.
      const frac = (progress - segStart) / (segEnd - segStart);
      to = {
        lat: s.from.lat + (s.to.lat - s.from.lat) * frac,
        lon: s.from.lon + (s.to.lon - s.from.lon) * frac,
      };
    }
    features.push({
      type: 'Feature',
      properties: { kind: s.kind, opacity },
      geometry: { type: 'LineString', coordinates: [[s.from.lon, s.from.lat], [to.lon, to.lat]] },
    });
  });
  return { type: 'FeatureCollection', features };
}

/// A one-point FeatureCollection for the riding "packet" dot.
function pointFC(p: LatLon): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: [{ type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [p.lon, p.lat] } }],
  };
}

/// `pos?` markers for unlocatable hops: one amber text label at the midpoint of
/// each dashed connector, shown once that segment has finished drawing. This is
/// the honest cue the hybrid-path design (cn84) promised — the unknown hop's
/// callsign with a `?`, never a fabricated pin.
function labelFC(segments: PathSegment[], progress: number, opacity: number): FeatureCollection {
  const total = segments.length;
  const features: unknown[] = [];
  segments.forEach((s, i) => {
    if (s.kind !== 'dashed' || !s.unknownLabels?.length) return;
    if (progress < (i + 1) / total) return; // wait until this segment is fully drawn
    features.push({
      type: 'Feature',
      properties: { label: `${s.unknownLabels.join('/')} ?`, opacity },
      geometry: {
        type: 'Point',
        coordinates: [(s.from.lon + s.to.lon) / 2, (s.from.lat + s.to.lat) / 2],
      },
    });
  });
  return { type: 'FeatureCollection', features };
}

/// Animated digipeat path (cn84). Two triggers, one honest resolution:
///   - HOVER a pin → paint the full path immediately (held until mouse-out).
///   - a newly-HEARD fix → animate that path once (draw-in → linger → fade),
///     aprs.fi-style, unless the operator is hovering (hover wins).
/// The animation timeline math lives in `pathTrace` (unit-tested); this component
/// is the thin maplibre shell that applies each frame to the line + dot sources.
function DigipeatPathLayer({
  positions,
  operator,
}: {
  positions: HeardPosition[];
  operator: LatLon | null;
}) {
  const map = useMapContext();
  useMapOverlay(map, PATH_SOURCE, { type: 'geojson', data: EMPTY_FC }, PATH_LAYERS);
  useMapOverlay(map, PATH_DOT_SOURCE, { type: 'geojson', data: EMPTY_FC }, PATH_DOT_LAYERS);
  useMapOverlay(map, PATH_LABEL_SOURCE, { type: 'geojson', data: EMPTY_FC }, PATH_LABEL_LAYERS);

  // Long-lived handlers read current data through refs (no re-subscribe per render).
  const byCallRef = useRef<Map<string, HeardPosition>>(new Map());
  const locatedRef = useRef<Map<string, LatLon>>(new Map());
  const operatorRef = useRef<LatLon | null>(operator);
  operatorRef.current = operator;
  useMemo(() => {
    const by = new Map<string, HeardPosition>();
    const loc = new Map<string, LatLon>();
    for (const p of positions) {
      by.set(p.call, p);
      loc.set(p.call, { lat: p.lat, lon: p.lon });
    }
    byCallRef.current = by;
    locatedRef.current = loc;
  }, [positions]);

  const hoverActiveRef = useRef(false);
  const liveRef = useRef<ActiveTrace | null>(null);
  const rafRef = useRef(0);

  const setSrc = (id: string, fc: FeatureCollection) => {
    if (!map) return;
    const s = map.getSource(id) as { setData?: (d: unknown) => void } | undefined;
    s?.setData?.(fc);
  };
  const clearPath = () => {
    setSrc(PATH_SOURCE, EMPTY_FC);
    setSrc(PATH_DOT_SOURCE, EMPTY_FC);
    setSrc(PATH_LABEL_SOURCE, EMPTY_FC);
  };
  const segmentsFor = (call: string): PathSegment[] | null => {
    const p = byCallRef.current.get(call);
    if (!p) return null;
    // An object/item pin plots the object, not the transmitter — its via-chain
    // belongs to a station at a different location, so tracing from here would
    // fabricate the RF source. Skip (Codex cn84 review, RF-honesty).
    if (p.isObject) return null;
    const segs = resolveDigipeatPath({
      src: { call: p.call, lat: p.lat, lon: p.lon },
      via: p.via ?? [],
      located: locatedRef.current,
      operator: operatorRef.current,
    });
    return segs.length ? segs : null;
  };

  // Hover: paint the full honest path immediately, held until mouse-out.
  useEffect(() => {
    if (!map) return;
    const enter = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call == null) return;
      const segs = segmentsFor(String(call));
      if (!segs) return;
      hoverActiveRef.current = true;
      setSrc(PATH_SOURCE, pathFC(segs, 1, 1));
      setSrc(PATH_DOT_SOURCE, EMPTY_FC);
      setSrc(PATH_LABEL_SOURCE, labelFC(segs, 1, 1));
    };
    const leave = () => {
      hoverActiveRef.current = false;
      clearPath();
    };
    map.on('mouseenter', POSITION_PINS_COLOR_LAYER, enter as (...a: unknown[]) => void);
    map.on('mouseleave', POSITION_PINS_COLOR_LAYER, leave as (...a: unknown[]) => void);
    return () => {
      map.off('mouseenter', POSITION_PINS_COLOR_LAYER, enter as (...a: unknown[]) => void);
      map.off('mouseleave', POSITION_PINS_COLOR_LAYER, leave as (...a: unknown[]) => void);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map]);

  // Live auto-trace: animate the newest fix's path once (unless hovering).
  const newest = positions.length ? positions.reduce((a, b) => (b.at > a.at ? b : a)) : null;
  const newestKey = newest ? `${newest.call}:${newest.at}` : '';
  useEffect(() => {
    if (!map || !newest || hoverActiveRef.current) return;
    const segs = segmentsFor(newest.call);
    if (!segs) return;
    liveRef.current = { segments: segs, startMs: performance.now(), mode: 'live', timings: DEFAULT_TIMINGS };
    cancelAnimationFrame(rafRef.current);
    const loop = () => {
      const active = liveRef.current;
      if (!active || hoverActiveRef.current) return; // hover owns the paint
      const f = computeTraceFrame(active, performance.now());
      if (f.phase === 'idle') {
        liveRef.current = null;
        clearPath();
        return;
      }
      setSrc(PATH_SOURCE, pathFC(f.segments, f.progress, f.opacity));
      setSrc(PATH_DOT_SOURCE, f.packet ? pointFC(f.packet) : EMPTY_FC);
      setSrc(PATH_LABEL_SOURCE, labelFC(f.segments, f.progress, f.opacity));
      rafRef.current = requestAnimationFrame(loop);
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map, newestKey]);

  return null;
}

/// The operator's own position pin ("you"). Sourced from the operator grid, not a
/// decoded beacon — drawn distinctly so it never reads as a heard station.
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

export function AprsPositionsMap({ positions, operatorGrid }: AprsPositionsMapProps) {
  const me = operatorGrid ? gridToLatLon(operatorGrid) : null;
  // tuxlink-dwzu: remember + restore the operator's last viewport so the map
  // opens where it was left. First run (no saved view) centers on the operator
  // at the local zoom — never the mid-Atlantic world view — falling back to the
  // world view only when no operator grid is known.
  const { saved, onViewportChange } = usePersistedViewport('tuxlink:map-viewport:aprs');
  const initialCenter = saved ? saved.center : (me ?? undefined);
  const initialZoom = saved ? saved.zoom : me ? OPERATOR_ZOOM : 2;
  return (
    <div className="aprs-positions-map" data-testid="aprs-positions-map">
      <MapLibreMap
        initialCenter={initialCenter}
        initialZoom={initialZoom}
        onViewportChange={onViewportChange}
      >
        <PositionLayers positions={positions} />
        <DigipeatPathLayer positions={positions} operator={me} />
        <OperatorPin location={me} />
        <RecenterControl target={me} zoom={OPERATOR_ZOOM} />
      </MapLibreMap>
    </div>
  );
}
