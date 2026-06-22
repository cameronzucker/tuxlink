/**
 * LocationMap — the offline location-setup map (tuxlink-yy1m), Leaflet edition.
 *
 * Leaflet re-expression (tuxlink-4hol; strangler-fig twin): composes LeafletMap +
 * LeafletMaidenheadGridLayer and draws its own overlay (grid square + a single
 * marker) as an L.rectangle + a draggable L.marker on an explicit L.svg()
 * renderer. Props + the GpsSourcePicker call site are unchanged.
 *
 * Behaviors (operator wire-walk flows), preserved 1:1:
 *  - GPS source selected + a live fix → marker sits at the PRECISE fix lat/lon
 *    ("you are here"); the live fix coords are local-display only, never broadcast.
 *  - Manual (or no fix) → marker at the grid-square center.
 *  - Click the map OR drag the marker → sets the location by hand (→ Manual),
 *    which is how the operator overrides a GPS fix (flow 3). Drag uses Leaflet's
 *    NATIVE draggable marker (no manual mousedown/move/up recipe needed): the
 *    marker commits its dropped lat/lon on `dragend`.
 *
 * Real render/drag smoothness is grim-verified; the tests prove wiring only.
 */
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { LeafletMap } from '../map/LeafletMap';
import { LeafletMaidenheadGridLayer } from '../map/LeafletMaidenheadGridLayer';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import { reportFrontendError } from '../frontendErrorLog';
import type { LatLon } from '../map/projection';

export interface LocationMapProps {
  /** Current grid — square highlight + manual-marker center. */
  grid: string;
  /** Raw live GPS fix coords for the precise marker, or null when no fresh fix. */
  fixLatLon: { lat: number; lon: number } | null;
  /** Picker selection id ('manual' | 'gpsd' | 'serial:...'). 'manual' → the
   *  marker follows the grid, so an arriving fix doesn't yank a hand-set pin. */
  selectedSource: string;
  /** Fired with the new grid when the operator clicks the map or drags the pin. */
  onGridChange: (grid: string) => void;
}

/** Grid-square half-widths (deg): 4-char = 2°×1°; 6-char = 5′×2.5′. */
const RECT_HALF = { lat6: 1.25 / 60, lon6: 2.5 / 60, lat4: 0.5, lon4: 1.0 };

/** Grid-square bounds [[south,west],[north,east]] for a grid + its centre. */
function squareBounds(grid: string, ll: LatLon): L.LatLngBoundsExpression {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? RECT_HALF.lat6 : RECT_HALF.lat4;
  const halfLon = is6 ? RECT_HALF.lon6 : RECT_HALF.lon4;
  return [
    [ll.lat - halfLat, ll.lon - halfLon],
    [ll.lat + halfLat, ll.lon + halfLon],
  ];
}

/** A green "you are here" marker (matches the MapLibre #5fd39a dot). */
function markerIcon(): L.DivIcon {
  const html =
    '<span class="location-pin" data-testid="location-pin" style="display:block;width:14px;height:14px;' +
    'border-radius:50%;background:#5fd39a;border:2px solid #0a1a2a;box-sizing:border-box"></span>';
  return L.divIcon({ className: 'location-pin-icon', html, iconSize: [14, 14], iconAnchor: [7, 7] });
}

function LocationOverlay({ grid, fixLatLon, selectedSource, onGridChange }: LocationMapProps) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  const onGridChangeRef = useRef(onGridChange);
  onGridChangeRef.current = onGridChange;

  const ll = grid ? gridToLatLon(grid) : null;
  const showFix = selectedSource !== 'manual' && fixLatLon != null;
  // The marker sits at the precise fix when a GPS source is active, else the grid
  // centre. Primitive lat/lon so a fresh object with the same coords doesn't churn.
  const markerLat = showFix ? fixLatLon!.lat : ll?.lat ?? null;
  const markerLon = showFix ? fixLatLon!.lon : ll?.lon ?? null;

  useEffect(() => {
    if (!map || !group) return;
    try {
      group.clearLayers();
      // Grid square (drawn first so the marker sits above it).
      if (grid && ll) {
        L.rectangle(squareBounds(grid, ll), {
          renderer: rendererRef.current ?? undefined,
          color: '#5fd39a',
          weight: 2,
          fillColor: '#5fd39a',
          fillOpacity: 0.1,
          interactive: false,
        }).addTo(group);
      }
      // Draggable marker → set location by hand (flow 3).
      if (markerLat != null && markerLon != null) {
        const marker = L.marker([markerLat, markerLon], {
          icon: markerIcon(),
          draggable: true,
          keyboard: false,
        });
        marker.on('dragend', () => {
          const p = marker.getLatLng();
          onGridChangeRef.current(latLonToGrid(p.lat, p.lng));
        });
        marker.addTo(group);
      }
    } catch (e) {
      reportFrontendError(
        'location-map',
        `overlay reconcile: ${e instanceof Error ? e.message : String(e)}`,
        e instanceof Error ? e.stack : undefined,
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- primitive marker coords + grid; onGridChange held in a ref
  }, [map, group, grid, ll?.lat, ll?.lon, markerLat, markerLon]);

  return null;
}

export function LocationMap({ grid, fixLatLon, selectedSource, onGridChange }: LocationMapProps) {
  const ll = grid ? gridToLatLon(grid) : null;
  const showFix = selectedSource !== 'manual' && fixLatLon != null;
  // Center the map ONCE, on the first known location (precise fix if a GPS source
  // is active, else the grid centre). The live fix afterward moves only the MARKER
  // (LocationOverlay) — not the camera — so the operator can pan freely to hand-set
  // location. Passing the live, per-tick `fixLatLon` as initialCenter made the map a
  // follow-cam that re-`flyTo`'d on every GPS update and fought the operator
  // (tuxlink-gf5s). Mirrors the stable-center discipline of StationFinderMap /
  // AprsPositionsMap.
  const initialCenterRef = useRef<LatLon | null>(null);
  if (initialCenterRef.current === null) {
    initialCenterRef.current = (showFix ? fixLatLon : ll) ?? null;
  }
  const center: LatLon | undefined = initialCenterRef.current ?? undefined;
  return (
    <div className="location-map" data-testid="location-map">
      <LeafletMap
        onMapClick={({ lat, lon }) => onGridChange(latLonToGrid(lat, lon))}
        initialCenter={center}
        initialZoom={center ? 6 : 2}
      >
        <LeafletMaidenheadGridLayer visible />
        <LocationOverlay
          grid={grid}
          fixLatLon={fixLatLon}
          selectedSource={selectedSource}
          onGridChange={onGridChange}
        />
      </LeafletMap>
    </div>
  );
}
