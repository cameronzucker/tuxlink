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
