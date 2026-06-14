// TypeScript binding for the operator's propagation preferences (tuxlink-s0r1).
// These shape the offline HF predictor: the operator's own antenna (TX end),
// the required SNR threshold, and TX power. Backed by the Rust commands
// `propagation_prefs_read` / `propagation_prefs_write` (src-tauri/src/propagation).

import { invoke } from '@tauri-apps/api/core';

/// Operator antenna preset. Kebab-case values mirror the Rust `AntennaPreset`
/// serde shape. Each maps to a VOACAP pattern model server-side.
export type AntennaPreset =
  | 'efhw-sloper'
  | 'portable-vertical-whip'
  | 'nvis-wire-dipole'
  | 'base-vertical-radials'
  | 'mobile-hf-whip'
  | 'random-wire-unun'
  | 'resonant-portable-dipole'
  | 'magnetic-loop'
  | 'beam-yagi'
  | 'unknown';

/// Ground electrical type under the operator's antenna. Kebab-case mirrors the
/// Rust `GroundType` serde shape; shapes the elevation pattern server-side.
export type GroundType = 'average' | 'sea-water' | 'good-soil' | 'poor-soil';

/// Man-made radio-noise environment (VOACAP SYSTEM-card noise level). Kebab-case
/// mirrors the Rust `NoiseEnvironment` serde shape.
export type NoiseEnvironment = 'city' | 'residential' | 'rural' | 'quiet-rural' | 'remote';

/// Clean camelCase prefs the UI works with.
export interface PropagationPrefs {
  antennaPreset: AntennaPreset;
  /** Required SNR for the reliability calc, dB-Hz. */
  reqSnrDb: number;
  /** TX power, watts. */
  txPowerW: number;
  /** Antenna height above ground, metres (drives the high-angle/NVIS pattern). */
  antennaHeightM: number;
  /** Ground type under the antenna. */
  groundType: GroundType;
  /** Local man-made radio-noise environment. */
  noiseEnvironment: NoiseEnvironment;
}

/// The serde wire shape returned by `propagation_prefs_read` (snake_case, no rename).
interface PropagationPrefsWire {
  antenna_preset: AntennaPreset;
  req_snr_db: number;
  tx_power_w: number;
  antenna_height_m: number;
  ground_type: GroundType;
  noise_environment: NoiseEnvironment;
}

/** Defaults mirror the Rust side (EFHW sloper, 38 dB-Hz unknown-mode SNR, 100 W).
 * 38 = VOACAP author's SSB anchor; mildly conservative vs VARA-HF reliable-connect
 * (~35-37 dB-Hz). See propagation/prefs.rs DEFAULT_REQ_SNR_DB + the recalibration
 * design note for the dB-Hz formula + per-mode table. */
export const DEFAULT_PROPAGATION_PREFS: PropagationPrefs = {
  antennaPreset: 'efhw-sloper',
  reqSnrDb: 38,
  txPowerW: 100,
  antennaHeightM: 9,
  groundType: 'average',
  noiseEnvironment: 'residential',
};

/// Ground-type options with operator labels. Order is the UI order.
export const GROUND_TYPE_OPTIONS: { value: GroundType; label: string; help: string }[] = [
  { value: 'average', label: 'Average soil', help: 'Typical ground (ε 13, σ 0.005). Default.' },
  { value: 'good-soil', label: 'Good / moist soil', help: 'Marsh, fertile or fresh-water-rich ground (ε 40, σ 0.02).' },
  { value: 'poor-soil', label: 'Poor / rocky / desert', help: 'Sandy, rocky or arid ground (ε 3, σ 0.001).' },
  { value: 'sea-water', label: 'Salt water', help: 'Coastal / over-water (ε 80, σ 5.0). Best low-angle ground.' },
];

/// Noise-environment options with operator labels (noisiest → quietest). Order is
/// the UI order. The local noise floor strongly affects predicted reliability.
export const NOISE_ENVIRONMENT_OPTIONS: { value: NoiseEnvironment; label: string; help: string }[] = [
  { value: 'city', label: 'City / industrial', help: 'Noisiest (-140 dBW). Dense urban, industrial, strong RFI.' },
  { value: 'residential', label: 'Residential / suburban', help: 'Suburban lot (-145 dBW). Default.' },
  { value: 'rural', label: 'Rural', help: 'Open rural (-150 dBW).' },
  { value: 'quiet-rural', label: 'Quiet rural', help: 'Low-RFI countryside (-155 dBW).' },
  { value: 'remote', label: 'Remote', help: 'Quietest (-164 dBW). Far from power lines / RFI.' },
];

/// Selectable presets with operator-facing labels + a one-line model note. Order
/// is the UI order; EFHW sloper leads as the default. Grounded in the Hamexandria
/// Winlink-antenna survey (dev/scratch/winlink-antenna-archetypes.md).
export const ANTENNA_PRESET_OPTIONS: { value: AntennaPreset; label: string; help: string }[] = [
  { value: 'efhw-sloper', label: 'End-fed half-wave (EFHW) / sloper', help: 'Horizontal or sloped wire. No overhead null — models regional and DX paths evenly. Default.' },
  { value: 'nvis-wire-dipole', label: 'Low NVIS wire dipole / OCFD', help: 'Low horizontal wire for regional short-skip. Favors high-angle paths.' },
  { value: 'resonant-portable-dipole', label: 'Portable dipole (linked / inverted-V)', help: 'Field horizontal dipole.' },
  { value: 'random-wire-unun', label: 'Random wire + 9:1 unun', help: 'End-fed long wire; mixed takeoff angle.' },
  { value: 'magnetic-loop', label: 'Magnetic loop', help: 'Small transmitting loop.' },
  { value: 'portable-vertical-whip', label: 'Portable vertical whip', help: 'Chameleon / Wolf River / MP1 class. Low-angle; weak overhead (NVIS).' },
  { value: 'base-vertical-radials', label: 'Base vertical + radials', help: 'Ground-mounted vertical. Low-angle DX; weak overhead (NVIS).' },
  { value: 'mobile-hf-whip', label: 'Mobile HF whip (screwdriver / Hamstick)', help: 'Short loaded vertical. Low-angle; weak overhead (NVIS).' },
  { value: 'beam-yagi', label: 'Beam / Yagi (directional)', help: 'Tower-mounted directional; low-angle DX.' },
  { value: 'unknown', label: 'Unknown / generic', help: 'Neutral model when the antenna is unspecified.' },
];

/** Read the operator's propagation prefs (defaults on a fresh install). */
export async function readPropagationPrefs(): Promise<PropagationPrefs> {
  const w = await invoke<PropagationPrefsWire>('propagation_prefs_read');
  return {
    antennaPreset: w.antenna_preset,
    reqSnrDb: w.req_snr_db,
    txPowerW: w.tx_power_w,
    antennaHeightM: w.antenna_height_m,
    groundType: w.ground_type,
    noiseEnvironment: w.noise_environment,
  };
}

/** Persist the operator's propagation prefs. Rejects an out-of-range SNR/power/height Rust-side. */
export async function writePropagationPrefs(prefs: PropagationPrefs): Promise<void> {
  await invoke('propagation_prefs_write', {
    antennaPreset: prefs.antennaPreset,
    reqSnrDb: prefs.reqSnrDb,
    txPowerW: prefs.txPowerW,
    antennaHeightM: prefs.antennaHeightM,
    groundType: prefs.groundType,
    noiseEnvironment: prefs.noiseEnvironment,
  });
}
