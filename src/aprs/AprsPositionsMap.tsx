// APRS Tac Chat positions map (tuxlink-6vgt). Plots the positions of stations
// HEARD on the open channel — one pin per station at its decoded lat/lon, with a
// callsign label and a comment popup. RF-honesty: every pin is a real, decoded
// fix (no estimated locations); a station appears only after its beacon is heard.
//
// Reuses the MapLibre stack (MapLibreMap + MapContext + the owned useMapOverlay
// hook) directly rather than StationFinderMap, which is hardwired to catalog
// Station[] + reachability tiers + Maidenhead-grid centroids. Pins are GeoJSON
// CIRCLE + SYMBOL (text) layers — CSP-safe, no per-pin DOM — mirroring the
// circle-layer pattern StationFinderMap established (tuxlink-ndi4).

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

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

// One circle layer paints every pin; one symbol layer draws the callsign label
// offset above the pin. Data-driven so a single layer pair covers all stations.
const POSITION_LAYERS = (
  [
    {
      id: POSITION_PINS_LAYER,
      type: 'circle',
      source: POSITIONS_SOURCE,
      paint: {
        'circle-radius': 7,
        'circle-color': '#2f86f0',
        'circle-opacity': 0.9,
        'circle-stroke-color': '#ffffff',
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

function buildPositionFC(positions: HeardPosition[]): FeatureCollection {
  const features: unknown[] = positions.map((p) => ({
    type: 'Feature',
    properties: { call: p.call, comment: p.comment },
    geometry: { type: 'Point', coordinates: [p.lon, p.lat] },
  }));
  return { type: 'FeatureCollection', features };
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
  const [popup, setPopup] = useState<{ call: string; comment: string } | null>(null);

  const byCall = useMemo(() => {
    const m = new Map<string, HeardPosition>();
    for (const p of positions) m.set(p.call, p);
    return m;
  }, [positions]);
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;

  const fc = useMemo(() => buildPositionFC(positions), [positions]);

  useMapOverlay(map, POSITIONS_SOURCE, { type: 'geojson', data: EMPTY_FC }, POSITION_LAYERS);
  usePushData(map, POSITIONS_SOURCE, fc);

  // Click a pin → show its callsign + comment in an inline popup overlay.
  useEffect(() => {
    if (!map) return;
    const onClick = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call == null) return;
      const p = byCallRef.current.get(String(call));
      if (p) setPopup({ call: p.call, comment: p.comment });
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
