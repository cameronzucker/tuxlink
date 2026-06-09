/**
 * tileSource — TypeScript mirror of the Rust LAN-tile wire types (Phases 0-6)
 * plus thin `invoke` wrappers for the four tile commands.
 *
 * Wire-shape contract (verbatim from the Rust serde):
 *   - Struct fields are camelCase: `{ url, crs, scheme, minZoom, maxZoom,
 *     cacheBudgetMb, attribution, label }` for `TileSource`; `{ kind, zoom,
 *     label, cachedAt }` for `TileSourceStatus`.
 *   - `crs` / `scheme` are PascalCase enum variants ("Geodetic" / "Xyz" /
 *     "Tms"), NOT camelCase — they cross the wire as their Rust variant names.
 *   - `StatusKind` is the kebab-case union (serde rename_all = "kebab-case").
 *
 * Commands (names verbatim): configure_tile_source, test_tile_source,
 * clear_tile_cache, tile_source_status. Tiles themselves are served over the
 * Tauri `tile` URI scheme (`tile://localhost/{z}/{x}/{y}`), NOT through invoke —
 * see TileLayerBridge.
 */
import { invoke } from '@tauri-apps/api/core';

/** Coordinate reference system of the tile source. Only Geodetic (EPSG:4326). */
export type TileCrs = 'Geodetic';

/** Tile addressing scheme: XYZ (Slippy, y top-down) or TMS (y flipped). */
export type TileScheme = 'Xyz' | 'Tms';

/**
 * Validation/serving state of the configured tile source.
 * - `bundled`     — no LAN source; the bundled raster is the only base.
 * - `lan-live`    — LAN source validated + reachable; tiles served live.
 * - `lan-cached`  — LAN source validated but offline; serving from cache.
 * - `partial`     — reachable but only some zoom levels present.
 * - `unreachable` — configured but the host is not answering.
 * - `incompatible`— reachable but CRS/scheme/zoom mismatch — cannot serve.
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
  crs: TileCrs;
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

/**
 * Minimum status zoom at which a validated LAN tile source backs the view
 * finely enough to justify 6-char (subsquare, ~5×2.5 arc-minute) Maidenhead
 * precision. Below this, the lattice is square-level (4-char) at best, so a
 * 6-char locator would imply precision the rendered substrate does not show.
 *
 * Chosen as 12: subsquares are 5'×2.5' (≈9.3×4.6 km at the equator); a slippy
 * tile at z12 spans ≈0.088° ≈ 9.8 km, so z12 is the first zoom where a single
 * tile is finer-grained than a subsquare cell — the point at which 6-char
 * precision is genuinely backed by what the operator sees.
 */
export const SIX_CHAR_MIN_ZOOM = 12;

/** A minimal map-view descriptor for the 6-char gate (just the zoom matters). */
export interface MapView {
  zoom: number;
}

/**
 * True ONLY when the view under the pin is backed by validated LAN tiles
 * (`lan-live` / `lan-cached`) AND zoomed to at least {@link SIX_CHAR_MIN_ZOOM}.
 * In every other case (bundled / partial / unreachable / incompatible, or
 * insufficient zoom) the answer is false and consumers fall back to 4-char.
 *
 * `status.zoom` (the validated max zoom of the source) AND the live `view.zoom`
 * must BOTH clear the threshold: a source validated to z16 still cannot back
 * 6-char precision if the operator is zoomed out to z6.
 */
export function sixCharAllowed(status: TileSourceStatus, view: MapView): boolean {
  const validated = status.kind === 'lan-live' || status.kind === 'lan-cached';
  if (!validated) return false;
  return status.zoom >= SIX_CHAR_MIN_ZOOM && view.zoom >= SIX_CHAR_MIN_ZOOM;
}
