/**
 * Owned MapLibre hook layer (tuxlink-ndi4, plan L1/A15).
 *
 * tuxlink drives MapLibre through `maplibre-gl` directly (NOT react-map-gl), via
 * this thin owned layer. "Thin" = a small API with LARGE lifecycle correctness:
 * the failure modes the react-leaflet→MapLibre swap introduces all live here.
 *
 * Canonical lifecycle contract (every add/remove obeys it):
 *   1. NEVER add before `isStyleLoaded()` — a source/layer added against an
 *      unloaded style is silently dropped.
 *   2. Re-add on `styledata`. It fires repeatedly and after every `setStyle`
 *      (the light↔dark swap), which drops all sources/layers — so the add must
 *      be IDEMPOTENT: guard `getLayer`/`getSource` before adding.
 *   3. Tolerate StrictMode double-invoke (production `main.tsx` keeps
 *      <StrictMode>): mount→cleanup→mount must converge to one source/layer.
 *   4. Teardown order: `removeLayer` BEFORE `removeSource` (removing a source a
 *      layer still references errors in MapLibre). This CANNOT come from hook
 *      ordering: React 19 runs effect cleanups in DECLARATION order, the same
 *      order as setup (verified empirically — `setup A, setup B, cleanup A,
 *      cleanup B`). Setup needs source-before-layer (`addLayer` requires its
 *      source), so a source-first declaration also cleans up source-first —
 *      the wrong order. So a lifecycle-COUPLED source + its layers MUST be one
 *      hook ([`useMapOverlay`]) whose single cleanup removes layers then source.
 *
 * Use [`useMapSource`] / [`useMapLayer`] only for INDEPENDENT management where
 * teardown order is moot: a layer drawn on the always-present basemap source, or
 * a source with no tuxlink-managed layers. For a self-contained overlay (the
 * Maidenhead grid, the drag-select fill) use [`useMapOverlay`].
 *
 * Live paint/spec updates (same id, changed color) are NOT handled here — that is
 * a `setPaintProperty` path added when a consumer needs it; this layer owns
 * presence + lifecycle, not per-property reactivity.
 */
import { useEffect, useRef } from 'react';

/** The structural subset of `maplibregl.Map` these hooks touch. The real Map
 * and the test double both satisfy it (kept narrow to avoid coupling to
 * maplibre-gl's full type and to keep the double honest). */
export interface MapHookHost {
  // maplibre-gl types this `boolean | void`; the hooks only use it in a boolean
  // context, so accept the wider type so a real Map satisfies the host.
  isStyleLoaded(): boolean | void;
  on(type: string, handler: (...args: unknown[]) => void): unknown;
  off(type: string, handler: (...args: unknown[]) => void): unknown;
  getLayer(id: string): unknown;
  // `object` (not Record<string,unknown>) so BOTH maplibre's strict
  // LayerSpecification/AddLayerObject and the test double satisfy the host.
  addLayer(spec: object, beforeId?: string): void;
  removeLayer(id: string): void;
  getSource(id: string): unknown;
  addSource(id: string, source: object): void;
  removeSource(id: string): void;
}

/**
 * Run a best-effort teardown mutation on the map (tuxlink-bal7).
 *
 * During unmount maplibre may have already destroyed the style (`map.remove()`
 * ran), after which `getLayer`/`getSource` throw `undefined is not an object
 * (evaluating 'this.style.getLayer')` — so the existence guard ITSELF throws,
 * which propagated out of the effect cleanup and crashed the app to the
 * ErrorBoundary (the Find-a-Station close-crash). Removing layers/sources is
 * pointless once the map is gone (they are destroyed with it), so a throw here
 * means "nothing to remove" and is safely swallowed.
 */
function safeTeardown(fn: () => void): void {
  try {
    fn();
  } catch {
    /* map/style already torn down — its layers + sources are gone with it */
  }
}

/**
 * Best-effort add of a source/layer (tuxlink-7jru).
 *
 * Previously every `ensure` gated on `if (!map.isStyleLoaded()) return`. But with
 * a large OFFLINE region-pack basemap, `isStyleLoaded()` can stay `false`
 * indefinitely (it requires every source's tiles + sprite/glyphs to be fully
 * loaded), so the gate never opened and EVERY overlay was silently dropped — the
 * Find-a-Station "no station pins / no operator pin" bug, root-caused live on the
 * running app. `addSource`/`addLayer` only need the style *loaded* (post-`load`),
 * a strictly lower bar than `isStyleLoaded()`. So attempt the add and let it
 * throw if the style genuinely isn't ready yet; a later `load`/`styledata`
 * retries. (Hence each `ensure` now also fires on `load`, not just `styledata`.)
 */
function safeEnsure(fn: () => void): void {
  try {
    fn();
  } catch {
    /* style not addable yet (pre-load) — a later 'load'/'styledata' retries */
  }
}

/**
 * Keep a GeoJSON/vector source present on `map` under `id` for the component's
 * lifetime, surviving style swaps. Call BEFORE any `useMapLayer` that references
 * this source so teardown order stays layer-then-source.
 */
export function useMapSource(
  map: MapHookHost | null,
  id: string,
  source: Record<string, unknown>,
): void {
  // Hold the latest spec without re-running the effect on every render; the
  // effect re-runs only when the map handle or id changes.
  const sourceRef = useRef(source);
  sourceRef.current = source;

  useEffect(() => {
    if (!map) return;
    const ensure = () => safeEnsure(() => {
      if (!map.getSource(id)) map.addSource(id, sourceRef.current);
    });
    ensure();
    map.on('load', ensure);
    map.on('styledata', ensure);
    return () => {
      safeTeardown(() => {
        map.off('load', ensure);
        map.off('styledata', ensure);
        if (map.getSource(id)) map.removeSource(id);
      });
    };
  }, [map, id]);
}

/**
 * Keep `layer` present on `map` for the component's lifetime, surviving style
 * swaps. `beforeId` controls draw order (insert beneath an existing layer).
 */
export function useMapLayer(
  map: MapHookHost | null,
  layer: Record<string, unknown> & { id: string },
  beforeId?: string,
): void {
  const layerRef = useRef(layer);
  layerRef.current = layer;
  const id = layer.id;

  useEffect(() => {
    if (!map) return;
    const ensure = () => safeEnsure(() => {
      if (!map.getLayer(id)) map.addLayer(layerRef.current, beforeId);
    });
    ensure();
    map.on('load', ensure);
    map.on('styledata', ensure);
    return () => {
      safeTeardown(() => {
        map.off('load', ensure);
        map.off('styledata', ensure);
        if (map.getLayer(id)) map.removeLayer(id);
      });
    };
  }, [map, id, beforeId]);
}

/**
 * Keep a source AND its layers present together for the component's lifetime,
 * surviving style swaps, with a teardown that removes layers BEFORE the source.
 *
 * This is the canonical primitive for a self-contained overlay (a GeoJSON source
 * with the layers that draw it). Because React cleans up effects in declaration
 * order, the layer-before-source teardown order can only be guaranteed inside ONE
 * effect's cleanup — which is what this hook provides. `layers` is applied in
 * array order (index 0 drawn first / lowest); pass them bottom-to-top.
 */
export function useMapOverlay(
  map: MapHookHost | null,
  id: string,
  source: Record<string, unknown>,
  layers: Array<Record<string, unknown> & { id: string }>,
): void {
  const ref = useRef({ source, layers });
  ref.current = { source, layers };

  useEffect(() => {
    if (!map) return;
    const ensure = () => safeEnsure(() => {
      const current = ref.current;
      if (!map.getSource(id)) map.addSource(id, current.source);
      for (const layer of current.layers) {
        if (!map.getLayer(layer.id)) map.addLayer(layer);
      }
    });
    ensure();
    map.on('load', ensure);
    map.on('styledata', ensure);
    return () => {
      safeTeardown(() => {
        map.off('load', ensure);
        map.off('styledata', ensure);
        // Layers BEFORE source: MapLibre errors removing a source still in use.
        for (const layer of ref.current.layers) {
          if (map.getLayer(layer.id)) map.removeLayer(layer.id);
        }
        if (map.getSource(id)) map.removeSource(id);
      });
    };
  }, [map, id]);
}
