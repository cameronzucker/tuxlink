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

/**
 * `basemap_download_pack` result (mirrors Rust `DownloadResult`): the installed
 * pack plus whether it is live immediately. `requiresRestart` is true only when
 * the pack installed durably but in-memory registration failed — it serves after
 * the next restart. The UI surfaces an honest "restart to use" notice in that
 * case and skips signalling the live map to add a not-yet-servable source.
 */
export interface DownloadResult extends InstalledPack {
  requiresRestart: boolean;
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

/** `basemap:download-progress` event payload (mirrors Rust `DownloadProgress`). */
export interface DownloadProgress {
  packId: string;
  /** Bytes written to the in-progress `.part` so far. */
  bytes: number;
  /** Expected total (the manifest `typical_bytes` estimate) — the bar denominator. */
  total: number;
}

/** `basemap:download-done` event payload (mirrors Rust `DownloadDone`). */
export interface DownloadDone {
  packId: string;
  ok: boolean;
  error: string | null;
}

/** Tauri event channels for the download progress UI (see useDownloadProgress). */
export const DOWNLOAD_PROGRESS_EVENT = 'basemap:download-progress';
export const DOWNLOAD_DONE_EVENT = 'basemap:download-done';

export const listPacks = () => invoke<PacksList>('basemap_list_packs');
export const getManifest = () => invoke<RegionManifest>('basemap_get_manifest');
export const refreshManifest = () => invoke<RegionManifest>('basemap_refresh_manifest');
export const downloadPack = (args: DownloadArgs) =>
  invoke<DownloadResult>('basemap_download_pack', { args });
export const deletePack = (id: string) => invoke<boolean>('basemap_delete_pack', { id });

/** Cancel an in-flight pack download. No-op if nothing is downloading that id. */
export const cancelDownload = (packId: string) =>
  invoke<void>('basemap_cancel_download', { packId });

/**
 * Deterministic pack id the backend derives for a download request — mirrors
 * Rust `basemap::packs::{tier_pack_id, continent_pack_id}`. The UI knows this id
 * from the args it sent, so Cancel can target it immediately, before the first
 * progress event latches it in the hook (see C5: cancel-before-first-event).
 *
 * Tier: `tier-{tier_id}-{lat_tok}-{lon_tok}` where each token is a compass
 * letter + rounded integer magnitude (lat n/s, lon e/w), e.g. `tier-wide-n34-w112`.
 * Continent: `continent-{continent_id}`.
 */
function coordToken(value: number, positive: string, negative: string): string {
  const mag = Math.round(Math.abs(value));
  const dir = value < 0 ? negative : positive;
  return `${dir}${mag}`;
}
export function packIdForArgs(args: DownloadArgs): string {
  if (args.kind === 'tier') {
    const latTok = coordToken(args.lat0, 'n', 's');
    const lonTok = coordToken(args.lon0, 'e', 'w');
    return `tier-${args.tier_id}-${latTok}-${lonTok}`;
  }
  return `continent-${args.continent_id}`;
}

/** Notify the live map that installed packs changed (after a download/delete). */
export function emitPacksChanged(): void {
  window.dispatchEvent(new CustomEvent(BASEMAP_PACKS_CHANGED_EVENT));
}
