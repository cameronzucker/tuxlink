/**
 * MaidenheadOverlay — draws the Maidenhead grid lattice + cell labels over the
 * map. Self-driving: it reads the current map bounds + zoom and redraws on
 * move/zoom, choosing field (20°×10°) vs square (2°×1°) granularity by zoom.
 * `visible` toggles it (default on).
 *
 * The line/label *geometry* is pure (gridGeometry.ts, jsdom-tested); this
 * component only maps that geometry to react-leaflet elements. Real rendering
 * is verified via grim — do NOT assert coordinates through the test mock (C1).
 *
 * `bounds`/`level` props override the map-derived values (controlled/testing).
 */
import { useMemo, useState } from 'react';
import { Polyline, Marker, useMap, useMapEvents } from 'react-leaflet';
import L from 'leaflet';
import { gridLines, GridLevel, type GridBounds } from './gridGeometry';

export interface MaidenheadOverlayProps {
  visible?: boolean;
  /** Override the visible bounds (else derived from the map). */
  bounds?: GridBounds;
  /** Override the grid level (else derived from the map zoom). */
  level?: GridLevel;
}

/** Coarser fields when zoomed out; squares once zoomed in. */
function levelFromZoom(zoom: number): GridLevel {
  return zoom >= 3 ? GridLevel.Square : GridLevel.Field;
}

const LINE_OPTIONS = { color: '#64748b', weight: 1, opacity: 0.5 };

export function MaidenheadOverlay({ visible = true, bounds, level }: MaidenheadOverlayProps) {
  const map = useMap();
  // Redraw on pan/zoom by bumping a tick (the map is the source of truth).
  const [, setTick] = useState(0);
  useMapEvents({
    moveend() {
      setTick((t) => t + 1);
    },
    zoomend() {
      setTick((t) => t + 1);
    },
  });

  const effBounds: GridBounds =
    bounds ??
    (() => {
      const b = map.getBounds();
      return { south: b.getSouth(), west: b.getWest(), north: b.getNorth(), east: b.getEast() };
    })();
  const effLevel: GridLevel = level ?? levelFromZoom(map.getZoom());

  const { lonLines, latLines, labels } = useMemo(
    () => gridLines(effBounds, effLevel),
    [effBounds.south, effBounds.west, effBounds.north, effBounds.east, effLevel],
  );

  if (!visible) return null;

  return (
    <>
      {lonLines.map((lon) => (
        <Polyline
          key={`lon-${lon}`}
          positions={[
            [effBounds.south, lon],
            [effBounds.north, lon],
          ]}
          pathOptions={LINE_OPTIONS}
          interactive={false}
        />
      ))}
      {latLines.map((lat) => (
        <Polyline
          key={`lat-${lat}`}
          positions={[
            [lat, effBounds.west],
            [lat, effBounds.east],
          ]}
          pathOptions={LINE_OPTIONS}
          interactive={false}
        />
      ))}
      {labels.map((lbl) => (
        <Marker
          key={`label-${lbl.lat}-${lbl.lon}`}
          position={[lbl.lat, lbl.lon]}
          icon={L.divIcon({ className: 'maidenhead-grid-label', html: lbl.text })}
          interactive={false}
        />
      ))}
    </>
  );
}
