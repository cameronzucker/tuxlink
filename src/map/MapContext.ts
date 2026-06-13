/**
 * Map context (tuxlink-ndi4, plan phase 2 / C11 re-expression).
 *
 * Leaflet overlays were react-leaflet CHILDREN that called `useMap()`. MapLibre
 * is driven imperatively, so overlays instead consume the live map from this
 * context and wire themselves via the owned hooks (`useMapOverlay` etc.). The
 * value is `null` until the map's `load` fires; the hooks tolerate a null map,
 * so overlays mount harmlessly before the map is ready and wire up once it is.
 */
import { createContext, useContext } from 'react';
import type { Map as MaplibreMap } from 'maplibre-gl';

const MapContext = createContext<MaplibreMap | null>(null);

export const MapProvider = MapContext.Provider;

/** The live MapLibre map, or `null` before it has loaded. */
export function useMapContext(): MaplibreMap | null {
  return useContext(MapContext);
}
