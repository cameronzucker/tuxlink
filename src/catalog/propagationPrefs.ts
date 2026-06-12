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

/// Clean camelCase prefs the UI works with.
export interface PropagationPrefs {
  antennaPreset: AntennaPreset;
  /** Required SNR for the reliability calc, dB-Hz. */
  reqSnrDb: number;
  /** TX power, watts. */
  txPowerW: number;
}

/// The serde wire shape returned by `propagation_prefs_read` (snake_case, no rename).
interface PropagationPrefsWire {
  antenna_preset: AntennaPreset;
  req_snr_db: number;
  tx_power_w: number;
}

/** Defaults mirror the Rust side (EFHW sloper, 22 dB-Hz data SNR, 100 W). */
export const DEFAULT_PROPAGATION_PREFS: PropagationPrefs = {
  antennaPreset: 'efhw-sloper',
  reqSnrDb: 22,
  txPowerW: 100,
};

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
  return { antennaPreset: w.antenna_preset, reqSnrDb: w.req_snr_db, txPowerW: w.tx_power_w };
}

/** Persist the operator's propagation prefs. Rejects an out-of-range SNR/power Rust-side. */
export async function writePropagationPrefs(prefs: PropagationPrefs): Promise<void> {
  await invoke('propagation_prefs_write', {
    antennaPreset: prefs.antennaPreset,
    reqSnrDb: prefs.reqSnrDb,
    txPowerW: prefs.txPowerW,
  });
}
