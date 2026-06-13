/**
 * Offline region-pack command wrappers + types (tuxlink-ndi4, phase 4).
 *
 * Thin typed bindings over the `basemap_*` Tauri commands (see
 * src-tauri/src/basemap/commands.rs) shared by the live map (MapLibreMap, which
 * composites installed packs) and the pack-manager UI (OfflineMapsSettings).
 */
import { invoke } from '@tauri-apps/api/core';

/** Window event the manager dispatches after a download/delete so the live map
 * re-fetches installed packs and recomposites its style. */
export const BASEMAP_PACKS_CHANGED_EVENT = 'tuxlink:basemap-packs-changed';

/** One installed region pack (mirrors Rust `basemap::packs::InstalledPack`). */
export interface InstalledPack {
  id: string;
  label: string;
  /** [west, south, east, north]. */
  bbox: [number, number, number, number];
  minzoom: number;
  maxzoom: number;
  schema_version: string;
  bytes: number;
  source_build: string;
  installed_at: string;
}

/** `basemap_list_packs` response. */
export interface PacksList {
  packs: InstalledPack[];
  total_bytes: number;
}

/** A coverage tier preset (mirrors Rust `region_manifest::Tier`). */
export interface Tier {
  id: string;
  label: string;
  /** [half-width-lon, half-width-lat] degrees. */
  half_deg: [number, number];
  typical_bytes: number;
  default?: boolean;
}

/** A named continent preset (mirrors Rust `region_manifest::Continent`). */
export interface Continent {
  id: string;
  label: string;
  bbox: [number, number, number, number];
  typical_bytes: number;
}

/** The region manifest (mirrors Rust `region_manifest::RegionManifest`). */
export interface RegionManifest {
  schema: string;
  planet_build: string;
  planet_url: string;
  pmtiles_schema: { planetiler_version: number; vector_layers: string[] };
  tiers: Tier[];
  continents: Continent[];
}

/** Download request: a tier centered on the operator grid, or a named continent. */
export type DownloadArgs =
  | { kind: 'tier'; tier_id: string; lon0: number; lat0: number }
  | { kind: 'continent'; continent_id: string };

export const listPacks = () => invoke<PacksList>('basemap_list_packs');
export const getManifest = () => invoke<RegionManifest>('basemap_get_manifest');
export const refreshManifest = () => invoke<RegionManifest>('basemap_refresh_manifest');
export const downloadPack = (args: DownloadArgs) =>
  invoke<InstalledPack>('basemap_download_pack', { args });
export const deletePack = (id: string) => invoke<boolean>('basemap_delete_pack', { id });

/** Notify the live map that installed packs changed (after a download/delete). */
export function emitPacksChanged(): void {
  window.dispatchEvent(new CustomEvent(BASEMAP_PACKS_CHANGED_EVENT));
}
