// TypeScript binding for U1's offline HF-prediction command (design §5).
// The Rust command `propagation_predict_path` (src-tauri/src/propagation/) takes
// the operator + station grids and a list of HF dials, and returns per-frequency
// 24-hour VOACAP reliability/SNR/MUFday. The only time input is the cached SSN
// (returned for provenance). Year/month are derived server-side from the UTC
// clock — the frontend passes only RF inputs.
//
// Degrade contract (F17): when the engine is not bundled (e.g. a .deb without
// voacapl), the command throws UiError::Unavailable; callers use isUnavailable()
// to fall back to distance-only ranking rather than surfacing an error.

import { invoke } from '@tauri-apps/api/core';
import type { GatewayAntenna } from './stationTypes';

export interface ChannelReliability {
  /** Exact input dial in kHz (carried by index, not re-derived). */
  frequencyKhz: number;
  /** Rounded MHz VOACAP actually computed at (informational). */
  voacapMhz: number;
  /** 24 reliability values 0..1, indexed by UTC hour 0..23. */
  relByHour: number[];
  /** 24 SNR values (dB), indexed by UTC hour. */
  snrByHour: number[];
  /** 24 MUFday values 0..1, indexed by UTC hour. */
  mufdayByHour: number[];
}

export interface PathPrediction {
  /** TX→RX great-circle bearing, degrees. */
  bearingDeg: number;
  /** Great-circle path distance, km. */
  distanceKm: number;
  /** Smoothed sunspot number used (provenance for "solar data N old"). */
  ssn: number;
  /** UTC year the prediction was computed for. */
  year: number;
  /** UTC month (1-12). */
  month: number;
  channels: ChannelReliability[];
}

/** Backend cap: VOACAP input deck holds at most 11 frequencies per run. */
const MAX_FREQUENCIES = 11;

export async function predictPath(
  txGrid: string,
  rxGrid: string,
  frequenciesKhz: number[],
  gatewayAntenna?: GatewayAntenna | null,
): Promise<PathPrediction> {
  // Tauri v2 maps these camelCase keys to the Rust snake_case params
  // (tx_grid / rx_grid / frequencies_khz / gateway_antenna). Explicit `await`
  // (not `return invoke(...)`) so a rejection's handler attaches synchronously —
  // bare thenable-adoption leaves a one-microtask gap that trips test-runner
  // unhandled-rejection guards even when the caller catches it.
  // `gatewayAntenna` is the station's parsed B/D/V code (null/undefined → the
  // backend models an isotropic far end, never a whip).
  return await invoke<PathPrediction>('propagation_predict_path', {
    txGrid,
    rxGrid,
    frequenciesKhz: frequenciesKhz.slice(0, MAX_FREQUENCIES),
    gatewayAntenna: gatewayAntenna ?? null,
  });
}

interface UiErrorShape {
  kind: string;
  reason?: string;
}

/** True when a thrown invoke error is the engine-not-available degrade signal. */
export function isUnavailable(err: unknown): boolean {
  return (
    typeof err === 'object' &&
    err !== null &&
    (err as UiErrorShape).kind === 'Unavailable'
  );
}
