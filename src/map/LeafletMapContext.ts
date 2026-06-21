/**
 * Leaflet map context (tuxlink-6kdw, plan phase 1 / Task 3).
 *
 * The Leaflet twin of `MapContext.ts`. During the strangler-fig migration this
 * coexists with the MapLibre `MapContext` — `AprsPositionsMap` (migrated) reads
 * the live `L.Map` from HERE, while the four un-migrated consumers still read the
 * `maplibregl.Map` from `MapContext`. The two contexts are deliberately separate
 * because the engines are different types.
 *
 * The value is `null` until the map is ready; overlays tolerate a null map (they
 * wire up via `useLeafletLayerGroup`, which is null-tolerant), so they mount
 * harmlessly before the map exists and wire once it does.
 */
import { createContext, useContext } from 'react';
import type { Map as LeafletMapInstance } from 'leaflet';

const LeafletMapContext = createContext<LeafletMapInstance | null>(null);

export const LeafletMapProvider = LeafletMapContext.Provider;

/** The live Leaflet map, or `null` before it is ready. */
export function useLeafletMap(): LeafletMapInstance | null {
  return useContext(LeafletMapContext);
}
