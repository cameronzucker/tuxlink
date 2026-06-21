/**
 * Owned Leaflet overlay-lifecycle hook (tuxlink-6kdw, plan phase 1 / Task 3).
 *
 * Deliberately MINIMAL — the Leaflet engine earns a large simplification over the
 * MapLibre `mapHooks.ts`. The MapLibre hooks were complex (idempotent add,
 * layer-before-source teardown, `styledata` re-subscription) ONLY because
 * `setStyle` (the light↔dark / pack swap) dropped every source + layer, so
 * overlays had to re-add themselves. Leaflet has no `setStyle`: a flavor/pack
 * swap replaces the BASE tile layer(s) and leaves overlay layers untouched. So
 * the only primitive worth owning is the lifecycle of one container `LayerGroup`;
 * consumers add/remove their own `L.marker`/`L.circle`/`L.polygon`/`L.featureGroup`
 * to it directly. Do NOT port the MapLibre `styledata` re-add machinery.
 */
import { useEffect, useState } from 'react';
import L from 'leaflet';

/**
 * Keep one `L.LayerGroup` attached to `map` for the calling component's lifetime
 * (added on mount, removed on unmount). Null-tolerant: returns `null` while the
 * map is `null` (e.g. before it is ready), and re-creates the group when a real
 * map arrives.
 */
export function useLeafletLayerGroup(map: L.Map | null): L.LayerGroup | null {
  const [group, setGroup] = useState<L.LayerGroup | null>(null);

  useEffect(() => {
    if (!map) {
      setGroup(null);
      return;
    }
    const lg = L.layerGroup().addTo(map);
    setGroup(lg);
    return () => {
      // Guard: the map may already be torn down (its own cleanup ran first).
      if (map.hasLayer(lg)) map.removeLayer(lg);
      setGroup(null);
    };
  }, [map]);

  return group;
}
