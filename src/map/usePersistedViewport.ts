// usePersistedViewport (tuxlink-dwzu) — remember the operator's last map
// viewport (center + zoom) per surface and restore it on the next mount, so a
// map opens exactly where it was left instead of the default world view followed
// by a laborious tile load + flyTo to the operator position.
//
// Storage is localStorage (transient view state, not operator config) keyed per
// surface, e.g. `tuxlink:map-viewport:aprs` / `…:station-finder`. The saved
// viewport is read ONCE at mount (it seeds the initial camera; it must not
// re-seed mid-session and fight the operator's panning). Writes are debounced so
// a drag/zoom settle persists once, not on every intermediate moveend.

import { useCallback, useRef } from 'react';
import type { LatLon } from './projection';

export interface SavedViewport {
  center: LatLon;
  zoom: number;
}

const DEBOUNCE_MS = 300;

/** Safely resolve localStorage. The `window.localStorage` GETTER itself can
 *  throw (storage disabled, opaque origin, strict privacy mode), so the access
 *  is wrapped — a blocked-storage webview must degrade to first-run, never crash
 *  the map render (Codex adrev P2). */
function getStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

/** Parse + VALIDATE a stored viewport. Returns null on absent / corrupt /
 *  out-of-range data — a cross-version or hand-edited value must degrade to the
 *  first-run fallback, never push a garbage camera into MapLibre. */
function readSaved(key: string): SavedViewport | null {
  const storage = getStorage();
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    if (!raw) return null;
    const v = JSON.parse(raw) as { center?: { lat?: unknown; lon?: unknown }; zoom?: unknown };
    const lat = v?.center?.lat;
    const lon = v?.center?.lon;
    const zoom = v?.zoom;
    if (
      typeof lat !== 'number' ||
      typeof lon !== 'number' ||
      typeof zoom !== 'number' ||
      !Number.isFinite(lat) ||
      !Number.isFinite(lon) ||
      !Number.isFinite(zoom)
    ) {
      return null;
    }
    if (lat < -90 || lat > 90 || lon < -180 || lon > 180) return null;
    // Generous zoom bound (the map clamps to its own min/max anyway); rejects
    // obviously corrupt values without coupling to MAP_MAX_ZOOM.
    if (zoom < 0 || zoom > 24) return null;
    return { center: { lat, lon }, zoom };
  } catch {
    return null;
  }
}

export interface UsePersistedViewport {
  /** The viewport saved at mount, or null on first run / corrupt storage. */
  saved: SavedViewport | null;
  /** Persist a new viewport (debounced). Pass to MapLibreMap's onViewportChange. */
  onViewportChange: (center: LatLon, zoom: number) => void;
}

export function usePersistedViewport(key: string): UsePersistedViewport {
  // Read ONCE at mount: the saved viewport seeds the initial camera and must not
  // change across renders (a re-read after a write would re-seed mid-session).
  const savedRef = useRef<SavedViewport | null | undefined>(undefined);
  if (savedRef.current === undefined) savedRef.current = readSaved(key);
  const saved = savedRef.current;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onViewportChange = useCallback(
    (center: LatLon, zoom: number) => {
      const storage = getStorage();
      if (!storage) return;
      if (!Number.isFinite(center.lat) || !Number.isFinite(center.lon) || !Number.isFinite(zoom)) {
        return;
      }
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        try {
          storage.setItem(
            key,
            JSON.stringify({ center: { lat: center.lat, lon: center.lon }, zoom }),
          );
        } catch {
          // Quota / private-mode / disabled storage — viewport memory is
          // best-effort; the map still works, it just won't restore.
        }
      }, DEBOUNCE_MS);
    },
    [key],
  );

  return { saved, onViewportChange };
}
