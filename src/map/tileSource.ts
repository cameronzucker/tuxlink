/**
 * tileSource — TypeScript mirror of the Rust LAN-tile wire types (Phases 0-6)
 * plus thin `invoke` wrappers for the four tile commands.
 *
 * Wire-shape contract (verbatim from the Rust serde):
 *   - Struct fields are camelCase: `{ url, scheme, minZoom, maxZoom,
 *     cacheBudgetMb, attribution, label }` for `TileSource`; `{ kind, zoom,
 *     label, cachedAt }` for `TileSourceStatus`.
 *   - `scheme` is a PascalCase enum variant ("Xyz" / "Tms"), NOT camelCase —
 *     it crosses the wire as the Rust variant name. Standard Web Mercator
 *     (EPSG:3857) is assumed; there is no `crs` field on the wire.
 *   - `StatusKind` is the kebab-case union (serde rename_all = "kebab-case").
 *
 * Commands (names verbatim): configure_tile_source, test_tile_source,
 * clear_tile_cache, tile_source_status. Tiles themselves are served over the
 * Tauri `tile` URI scheme (`tile://localhost/{z}/{x}/{y}`), NOT through invoke —
 * see TileLayerBridge.
 */
import { invoke } from '@tauri-apps/api/core';

/** Tile addressing scheme: XYZ (Slippy, y top-down) or TMS (y flipped). */
export type TileScheme = 'Xyz' | 'Tms';

/**
 * Validation/serving state of the configured tile source.
 * - `bundled`     — no LAN source; the bundled raster is the only base.
 * - `lan-live`    — LAN source validated + reachable; tiles served live.
 * - `lan-cached`  — LAN source validated but offline; serving from cache.
 * - `partial`     — reachable but only some zoom levels present.
 * - `unreachable` — configured but the host is not answering.
 * - `incompatible`— reachable but the server didn't return standard image tiles / scheme or zoom mismatch.
 */
export type StatusKind =
  | 'bundled'
  | 'lan-live'
  | 'lan-cached'
  | 'partial'
  | 'unreachable'
  | 'incompatible';

/** A LAN tile source. Field names + casing mirror the Rust serde exactly. */
export interface TileSource {
  url: string;
  scheme: TileScheme;
  minZoom: number;
  maxZoom: number;
  cacheBudgetMb: number;
  attribution: string | null;
  label: string;
}

/** Current tile-source status reported by the backend. */
export interface TileSourceStatus {
  kind: StatusKind;
  zoom: number;
  label: string | null;
  cachedAt: string | null;
}

/** Configure (persist + validate) a LAN tile source; returns the new status. */
export function configureTileSource(source: TileSource): Promise<TileSourceStatus> {
  return invoke<TileSourceStatus>('configure_tile_source', { source });
}

/** Probe a candidate source WITHOUT persisting it; returns the probe status. */
export function testTileSource(source: TileSource): Promise<TileSourceStatus> {
  return invoke<TileSourceStatus>('test_tile_source', { source });
}

/** Drop the on-disk tile cache for the configured source. */
export function clearTileCache(): Promise<void> {
  return invoke<void>('clear_tile_cache');
}

/** Fetch the current tile-source status. */
export function getTileSourceStatus(): Promise<TileSourceStatus> {
  return invoke<TileSourceStatus>('tile_source_status');
}

// ── 6-char precision gate (Phase 7.5) ──────────────────────────────────────

