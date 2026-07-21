// Shared frequency parsing + normalization for the radio panels (tuxlink-8fkkk
// C4). Both ARDOP and VARA panels accept an operator-entered MHz field and
// consume a dial's freq metadata; this is the single source of truth for both
// directions so the panels never drift on the kHz-vs-MHz normalization.

import type { FavoriteDial } from '../../favorites/types';

/**
 * Parse the operator's MHz text field into Hz. "7.102" → 7102000. Empty,
 * non-numeric, or non-positive input → null (the backend skips the CAT retune
 * for a null freq). This is the inverse of what the panel shows in the field.
 */
export function parseFreqInputToHz(input: string): number | null {
  const t = input.trim();
  if (!t) return null;
  const mhz = Number(t);
  if (!Number.isFinite(mhz) || mhz <= 0) return null;
  return Math.round(mhz * 1_000_000);
}

/**
 * Format a raw Hz value (e.g. a peer `Channel.freq_hz`) as the MHz string the
 * freq input field expects. `7102000` → `"7.102"`. This is the peer-prefill
 * counterpart of `dialFreqToMhzString` below — a peer channel's freq arrives
 * as a plain Hz number (`contacts/types.ts::Channel.freq_hz`), never as the
 * dial-metadata string `dialFreqToMhzString` parses, so it needs its own
 * (much simpler) formatter rather than round-tripping through a fake dial.
 */
export function hzToMhzString(hz: number): string {
  return String(Number((hz / 1_000_000).toFixed(6)));
}

/**
 * Normalize a dial's freq metadata into the MHz string the input field expects.
 *
 * The dials that reach a panel carry their freq in TWO magnitudes:
 *   - A Find-a-Station dial (`channelToDial`) carries a MHz string ("7.103").
 *   - A SAVED FAVORITE may carry kHz from an older record path ("14105.0").
 *
 * Heuristic by magnitude: parse the leading numeric; a value ≥ 1000 is kHz
 * (÷1000 → MHz), otherwise MHz verbatim. Favorites store kHz or MHz — never Hz
 * — so the ≥1000 ⇒ kHz rule is correct for the real data. A raw Hz string
 * ("7102000") is out of scope and would be (mis)treated as kHz under this rule;
 * no production dial carries Hz.
 *
 * Returns the MHz string for the field, or null when no parseable freq is
 * present (the caller then CLEARS the field — the C4 clear-on-empty fix).
 */
export function dialFreqToMhzString(dial: FavoriteDial): string | null {
  return freqStringToCanonicalMhz(dial.freq);
}

/**
 * The magnitude heuristic itself, extracted (tuxlink-ixasg) so `favoriteKey`
 * keys favorites through the SAME canonicalization the panels use — a kHz
 * record ("14105.0") and a MHz dial ("14.105") are one physical channel and
 * must produce one key. Any change here changes favorite identity: keep the
 * two consumers (this file's field normalization + favoriteKey) in mind.
 */
export function freqStringToCanonicalMhz(raw: string | undefined): string | null {
  if (!raw) return null;
  // Pull the first numeric token so "7.103 MHz" and "14105.0 kHz" both parse.
  const m = raw.match(/[\d.]+/);
  if (!m) return null;
  const n = Number(m[0]);
  if (!Number.isFinite(n) || n <= 0) return null;
  const mhz = n >= 1000 ? n / 1000 : n;
  // Drop trailing zeros so "14105.0" kHz → "14.105", "7.103" MHz → "7.103".
  return String(Number(mhz.toFixed(6)));
}

/** One QSY candidate as the backend's `DialCandidate` deserializes it. The
 *  serde derive on `modem_commands::DialCandidate` has NO `rename_all`, so the
 *  nested field is snake_case `freq_hz` (the top-level `qsyCandidates` arg name
 *  is camelCased by Tauri; nested struct fields are NOT). `freq_hz` is `null`
 *  when the dial carries no parseable freq (the backend skips the per-candidate
 *  tune). */
export interface QsyCandidate {
  target: string;
  freq_hz: number | null;
}

/** Map ranked dials to the backend `qsyCandidates` payload (tuxlink-8fkkk Task
 *  B). The target is the dial's gateway; the freq is parsed from the dial's
 *  (normalized) MHz string back to Hz. */
export function dialsToQsyCandidates(dials: FavoriteDial[]): QsyCandidate[] {
  return dials.map((d) => {
    const mhz = dialFreqToMhzString(d);
    return {
      target: d.gateway,
      freq_hz: mhz != null ? parseFreqInputToHz(mhz) : null,
    };
  });
}
