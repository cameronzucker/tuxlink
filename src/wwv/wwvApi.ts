// TypeScript bindings for the off-air WWV decode commands
// (src-tauri/src/wwv_offair/commands.rs). These tune the operator's rig to
// WWV, capture the next voice bulletin, transcribe it locally (Whisper), and
// ingest the parsed solar indices into the propagation forecast — no
// internet, no radio-network hop (RX-only; never transmits).
//
// Field-casing note (mirrors propagationApi.ts's convention, inverted):
// Tauri v2 maps camelCase JS invoke ARGS to the Rust command's snake_case
// params (nowMs -> now_ms), same as propagationApi.ts's txGrid -> tx_grid.
// But unlike PathPrediction (which carries `#[serde(rename_all = "camelCase")]`),
// WwvRefreshOutcome and SolarSnapshot have NO rename attribute in
// src-tauri/src/wwv_offair/commands.rs / src-tauri/src/propagation/solar_update.rs
// — they serialize with serde's default (as-written) casing, which is
// snake_case. So the RETURNED struct fields below stay snake_case
// (no_copy, updated_at_ms, a_index, k_index, forecast_updated) while the
// invoke() ARGS object stays camelCase.

import { invoke } from '@tauri-apps/api/core';

export interface SolarIndices {
  sfi: number;
  a_index?: number;
  k_index?: number;
}

export interface WwvRefreshOutcome {
  updated: boolean;
  indices: SolarIndices | null;
  source: string;
  no_copy: boolean;
  wav_path: string | null;
}

export interface SolarSnapshot {
  indices: SolarIndices | null;
  updated_at_ms: number;
  source: string;
  forecast_updated: boolean;
}

/** Capture the next WWV bulletin off-air and ingest it into the propagation forecast. */
export async function refreshOffair(nowMs: number): Promise<WwvRefreshOutcome> {
  return await invoke<WwvRefreshOutcome>('wwv_offair_refresh', { nowMs });
}

/** Read the last-persisted solar snapshot, whatever its source (SWPC, RF, or off-air). */
export async function readSnapshot(): Promise<SolarSnapshot | null> {
  return await invoke<SolarSnapshot | null>('wwv_offair_snapshot_read');
}

/** Read a captured off-air clip's raw WAV bytes for local playback (e.g. a no-copy capture). */
export async function readClip(path: string): Promise<Uint8Array> {
  const bytes = await invoke<number[]>('wwv_offair_read_clip', { path });
  return new Uint8Array(bytes);
}

/** Manually ingest solar indices heard/read by ear when the automatic decode had no copy. */
export async function manualIngest(
  sfi: number,
  aIndex: number | null,
  kIndex: number | null,
  nowMs: number,
): Promise<WwvRefreshOutcome> {
  return await invoke<WwvRefreshOutcome>('wwv_offair_manual_ingest', { sfi, aIndex, kIndex, nowMs });
}

/** Whether the operator's rig is CAT-controlled so an armed capture can auto-tune to WWV. */
export async function catConfigured(): Promise<boolean> {
  return await invoke<boolean>('wwv_offair_cat_configured');
}

/** Delete a kept no-copy capture WAV once it's no longer needed (manual entry done, or re-armed). */
export async function discardClip(path: string): Promise<void> {
  await invoke('wwv_offair_discard_clip', { path });
}
