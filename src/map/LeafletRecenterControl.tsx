// Recenter-on-operator control for the Leaflet substrate (tuxlink-6kdw). The
// Leaflet twin of RecenterControl: identical markup/testid/CSS, but reads the
// live L.Map from LeafletMapContext and calls Leaflet's flyTo([lat, lon], {...})
// (Leaflet uses [lat, lng] order; MapLibre used {center:[lng,lat]}). Kept as a
// separate component so the shared MapLibre RecenterControl stays untouched for
// the four un-migrated consumers during the strangler-fig migration. Hidden when
// no operator position is known.

import { useLeafletMap } from './LeafletMapContext';
import type { LatLon } from './projection';
import './RecenterControl.css';

export interface LeafletRecenterControlProps {
  /** Operator position to fly to, or null when unknown (control is hidden). */
  target: LatLon | null;
  /** Zoom to settle at when recentering. */
  zoom: number;
  /** Accessible label / tooltip. */
  label?: string;
}

export function LeafletRecenterControl({
  target,
  zoom,
  label = 'Center on my position',
}: LeafletRecenterControlProps) {
  const map = useLeafletMap();
  if (!target) return null;
  return (
    <button
      type="button"
      className="map-recenter-control"
      data-testid="map-recenter"
      aria-label={label}
      title={label}
      onClick={() => map?.flyTo([target.lat, target.lon], zoom)}
    >
      ⌖
    </button>
  );
}
