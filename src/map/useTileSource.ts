/**
 * useTileSource — reads the operator's persisted LAN tile source from config
 * and fetches its live status, returning a combined descriptor for BaseMap.
 *
 * Returns `{ source, status }` when ALL of the following hold:
 *   - `config_read` returns a non-null `map_tile_source`
 *   - `tile_source_status` reports a tile-backed status: `lan-live`, `lan-cached`,
 *     or `partial` (§8.5: partial is a live source with some 404s, not a fallback)
 *
 * Returns `null` in every other case — no source configured, source unreachable,
 * source incompatible, or any IPC error. Null is the safe offline fallback: a
 * BaseMap that receives no `tileSource` renders the bundled raster at maxZoom 3,
 * which is always correct even without a network.
 *
 * Error handling: all IPC errors are caught and silently mapped to null.
 * This hook NEVER throws; a map must render even when the backend is
 * unreachable at startup.
 *
 * Mounted-guard pattern mirrors PositionPickerOverlay.tsx ~line 77.
 */
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getTileSourceStatus, type TileSource, type TileSourceStatus } from './tileSource';

/** Status kinds that back a live tile layer + raised zoom cap. */
const TILE_BACKED_KINDS: ReadonlySet<TileSourceStatus['kind']> = new Set([
  'lan-live',
  'lan-cached',
  'partial',
]);

export interface TileSourceDescriptor {
  source: TileSource;
  status: TileSourceStatus;
}

/**
 * Reads the persisted LAN tile source (via `config_read` → `map_tile_source`)
 * and its live status (via `tile_source_status`). Returns a descriptor when
 * the source is present and tile-backed; returns null otherwise.
 *
 * Stable across unmount — uses a mounted guard to suppress setState after the
 * component has been removed.
 */
export function useTileSource(): TileSourceDescriptor | null {
  const [descriptor, setDescriptor] = useState<TileSourceDescriptor | null>(null);

  useEffect(() => {
    let mounted = true;

    const load = async () => {
      try {
        const config = await invoke<{ map_tile_source?: TileSource | null }>('config_read');
        const source = config?.map_tile_source ?? null;
        if (!source) {
          if (mounted) setDescriptor(null);
          return;
        }

        const status = await getTileSourceStatus();
        if (!mounted) return;

        if (TILE_BACKED_KINDS.has(status.kind)) {
          setDescriptor({ source, status });
        } else {
          setDescriptor(null);
        }
      } catch {
        if (mounted) setDescriptor(null);
      }
    };

    load();

    return () => {
      mounted = false;
    };
  }, []);

  return descriptor;
}
