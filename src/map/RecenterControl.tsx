// Recenter-on-operator control (tuxlink-dwzu). A compact button overlaid on the
// map that flies the camera back to the operator's position — the standard
// "find me" affordance. Because viewport-restore opens the map where it was last
// left, this is the one-tap way back to your own position. Hidden when no
// operator position is known. Lives as a child of MapLibreMap so it reads the
// live map via MapContext.

import { useMapContext } from './MapContext';
import type { LatLon } from './projection';
import './RecenterControl.css';

export interface RecenterControlProps {
  /** Operator position to fly to, or null when unknown (control is hidden). */
  target: LatLon | null;
  /** Zoom to settle at when recentering. */
  zoom: number;
  /** Accessible label / tooltip. */
  label?: string;
}

export function RecenterControl({ target, zoom, label = 'Center on my position' }: RecenterControlProps) {
  const map = useMapContext();
  if (!target) return null;
  return (
    <button
      type="button"
      className="map-recenter-control"
      data-testid="map-recenter"
      aria-label={label}
      title={label}
      onClick={() => map?.flyTo({ center: [target.lon, target.lat], zoom })}
    >
      ⌖
    </button>
  );
}
