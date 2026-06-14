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
import type { HeardPosition } from './aprsTypes';
import './AprsPositionsMap.css';

export interface AprsPositionsMapProps {
  /// Heard stations' latest positions (one per callsign), from useAprsPositions.
  positions: HeardPosition[];
}

const POSITIONS_SOURCE = 'aprs-positions';
const POSITION_PINS_LAYER = 'aprs-position-pins';
const POSITION_LABELS_LAYER = 'aprs-position-labels';
const UNCERTAINTY_SOURCE = 'aprs-position-uncertainty';
const UNCERTAINTY_FILL_LAYER = 'aprs-position-uncertainty-fill';
const UNCERTAINTY_LINE_LAYER = 'aprs-position-uncertainty-outline';

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

/// Radius, in metres, of the uncertainty region for a given ambiguity level.
/// `0` for a full-precision fix (level 0) — no region is drawn.
export function ambiguityRadiusMeters(level: number): number {
  const l = Math.max(0, Math.min(4, Math.floor(level)));
  return AMBIGUITY_HALF_MINUTES[l] * METERS_PER_MINUTE_LAT;
}

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

// One circle layer paints every pin; one symbol layer draws the callsign label
// offset above the pin. Data-driven so a single layer pair covers all stations:
// stale pins dim; ambiguous pins get an amber ring marking them as approximate.
const POSITION_LAYERS = (
  [
    {
      id: POSITION_PINS_LAYER,
      type: 'circle',
      source: POSITIONS_SOURCE,
      paint: {
        'circle-radius': 7,
        'circle-color': ['case', ['get', 'stale'], '#7d8794', '#2f86f0'],
        'circle-opacity': ['case', ['get', 'stale'], 0.5, 0.9],
        'circle-stroke-color': ['case', ['>', ['get', 'ambiguity'], 0], '#f0c24a', '#ffffff'],
        'circle-stroke-width': 1.5,
      },
    },
    {
      id: POSITION_LABELS_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: {
        'text-field': ['get', 'call'],
        'text-size': 11,
        'text-offset': [0, -1.2],
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

/// A ring of [lon,lat] points approximating a circle of `radiusM` metres around
/// the centre. Closed (first point repeated) so it forms a valid GeoJSON polygon.
function circlePolygon(lon: number, lat: number, radiusM: number, steps = 48): number[][] {
  const dLat = radiusM / 111320;
  const dLon = radiusM / (111320 * Math.cos((lat * Math.PI) / 180));
  const ring: number[][] = [];
  for (let i = 0; i <= steps; i++) {
    const theta = (i / steps) * 2 * Math.PI;
    ring.push([lon + dLon * Math.cos(theta), lat + dLat * Math.sin(theta)]);
  }
  return ring;
}

function buildPositionFC(positions: HeardPosition[], now: number): FeatureCollection {
  const features: unknown[] = positions.map((p) => ({
    type: 'Feature',
    properties: {
      call: p.call,
      comment: p.comment,
      ambiguity: p.ambiguity,
      stale: now - p.at > STALE_MS,
    },
    geometry: { type: 'Point', coordinates: [p.lon, p.lat] },
  }));
  return { type: 'FeatureCollection', features };
}

/// Uncertainty regions for ambiguous fixes only — a full-precision fix gets no
/// halo, so the map never implies uncertainty the wire did not report.
function buildUncertaintyFC(positions: HeardPosition[]): FeatureCollection {
  const features: unknown[] = positions
    .filter((p) => p.ambiguity > 0)
    .map((p) => ({
      type: 'Feature',
      properties: { call: p.call, ambiguity: p.ambiguity },
      geometry: {
        type: 'Polygon',
        coordinates: [circlePolygon(p.lon, p.lat, ambiguityRadiusMeters(p.ambiguity))],
      },
    }));
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

interface PopupState {
  call: string;
  comment: string;
  ambiguity: number;
  at: number;
}

/** Pushes GeoJSON to the source on change, re-pushing on styledata (style swap). */
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

function PositionLayers({ positions }: AprsPositionsMapProps) {
  const map = useMapContext();
  const [popup, setPopup] = useState<PopupState | null>(null);

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

  const fc = useMemo(() => buildPositionFC(positions, now), [positions, now]);
  const uncertaintyFc = useMemo(() => buildUncertaintyFC(positions), [positions]);

  // Uncertainty regions register first so the pins + labels draw on top of them.
  useMapOverlay(map, UNCERTAINTY_SOURCE, { type: 'geojson', data: EMPTY_FC }, UNCERTAINTY_LAYERS);
  useMapOverlay(map, POSITIONS_SOURCE, { type: 'geojson', data: EMPTY_FC }, POSITION_LAYERS);
  usePushData(map, UNCERTAINTY_SOURCE, uncertaintyFc);
  usePushData(map, POSITIONS_SOURCE, fc);

  // Click a pin → show its callsign + comment + last-heard age in an inline popup.
  useEffect(() => {
    if (!map) return;
    const onClick = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call == null) return;
      const p = byCallRef.current.get(String(call));
      if (p) setPopup({ call: p.call, comment: p.comment, ambiguity: p.ambiguity, at: p.at });
    };
    map.on('click', POSITION_PINS_LAYER, onClick as (...a: unknown[]) => void);
    return () => {
      map.off('click', POSITION_PINS_LAYER, onClick as (...a: unknown[]) => void);
    };
  }, [map]);

  if (!popup) return null;
  return (
    <div className="aprs-positions-map__popup" role="status" data-testid="aprs-position-popup">
      <button
        type="button"
        className="aprs-positions-map__popup-close"
        aria-label="Dismiss"
        onClick={() => setPopup(null)}
      >
        ×
      </button>
      <span className="aprs-positions-map__popup-call">{popup.call}</span>
      <span className="aprs-positions-map__popup-age" data-testid="aprs-position-age">
        last heard {formatAge(Math.max(0, now - popup.at))}
      </span>
      {popup.ambiguity > 0 && (
        <span className="aprs-positions-map__popup-ambiguity" data-testid="aprs-position-ambiguity">
          {ambiguityNote(popup.ambiguity)}
        </span>
      )}
      {popup.comment && <span className="aprs-positions-map__popup-comment">{popup.comment}</span>}
    </div>
  );
}

export function AprsPositionsMap({ positions }: AprsPositionsMapProps) {
  return (
    <div className="aprs-positions-map" data-testid="aprs-positions-map">
      <MapLibreMap>
        <PositionLayers positions={positions} />
      </MapLibreMap>
    </div>
  );
}
