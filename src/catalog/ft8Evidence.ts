// FT-8 evidence corroboration (Station Intelligence redesign, spec §2).
//
// Pure math: decides which catalog Stations are "corroborated" by recent FT-8
// decode evidence. No UI here, Task 7 wires this into the panel.
//
// THE FIXTURE AT `__fixtures__/evidence/basic.json` IS A CROSS-LANGUAGE
// CONTRACT: it is byte-identically duplicated into the Rust fixture dir in
// Task 12, and the Rust implementation must reproduce `expectCorroborated`
// exactly. Grid literals in that fixture were verified with `distanceFromGrids`
// (see the Task 6 report) so each of the four station cases fails or passes
// for exactly one reason, with comfortable margin from every threshold.

import type { Station } from './stationModel';
import { stationKey } from './useReachabilityMap';
import { distanceFromGrids, kmToMi } from './distance';
import type { SlotRecord } from '../ft8ui/ft8Types';

/** A decode more than this old relative to `opts.nowMs` cannot corroborate anything. */
export const EVIDENCE_RECENCY_MS = 30 * 60 * 1000;

/** Default SNR floor a caller (the UI) should start the threshold at. */
export const EVIDENCE_SNR_MIN_DB_DEFAULT = -24;

/** `evidenceRadiusMi` scales the operator-to-decode distance by this factor. */
export const EVIDENCE_RADIUS_FACTOR = 0.15;

/** `evidenceRadiusMi` floor, miles. */
export const EVIDENCE_RADIUS_MIN_MI = 50;

/** `evidenceRadiusMi` cap, miles. */
export const EVIDENCE_RADIUS_MAX_MI = 750;

export interface EvidenceOptions {
  /** Unix millis "now": the reference point for the recency window. */
  nowMs: number;
  /** Caller-supplied SNR floor (the UI threshold; not necessarily the default). */
  snrMinDb: number;
  /** The operator's own Maidenhead grid. */
  operatorGrid: string;
}

export interface EvidenceResult {
  /** stationKey(s) of every station corroborated by at least one qualifying decode. */
  corroborated: ReadonlySet<string>;
  /** Bands with >= 1 qualifying decode in-window (independent of any station match). */
  sampledBands: string[];
  /** Stations evaluated (i.e. `stations.length`). */
  considered: number;
}

/**
 * A decode's plausible corroboration radius scales with how far the operator
 * heard the SAME decode: a short operator↔decode path implies a tight local
 * band opening (radius floors at EVIDENCE_RADIUS_MIN_MI), while a long DX path
 * implies conditions could plausibly carry evidence over a wider radius too
 * (capped at EVIDENCE_RADIUS_MAX_MI so a single very-long DX contact can't
 * "corroborate" the entire continent).
 */
export function evidenceRadiusMi(operatorToHeardMi: number): number {
  const raw = EVIDENCE_RADIUS_FACTOR * operatorToHeardMi;
  return Math.min(EVIDENCE_RADIUS_MAX_MI, Math.max(EVIDENCE_RADIUS_MIN_MI, raw));
}

/** A decode that passed the grid/recency/SNR gate, with its operator distance
 *  memoized once (computed per decode, not per decode×station). */
interface QualifyingDecode {
  band: string;
  grid: string;
  operatorDistMi: number;
}

function collectQualifyingDecodes(ring: SlotRecord[], opts: EvidenceOptions): QualifyingDecode[] {
  const out: QualifyingDecode[] = [];
  for (const slot of ring) {
    for (const decode of slot.decodes) {
      if (!decode.grid) continue; // ungridded decodes can't be geo-corroborated
      if (opts.nowMs - decode.slotUtcMs > EVIDENCE_RECENCY_MS) continue; // stale
      if (decode.snrDb < opts.snrMinDb) continue; // below caller's SNR floor

      // distanceFromGrids null-guards through gridToLatLon: an unparseable
      // operatorGrid or decode.grid yields null here and the decode is dropped
      // (it cannot anchor a radius or reach a station either way).
      const operatorDistKm = distanceFromGrids(opts.operatorGrid, decode.grid);
      if (operatorDistKm == null) continue;

      out.push({ band: slot.band, grid: decode.grid, operatorDistMi: kmToMi(operatorDistKm) });
    }
  }
  return out;
}

/**
 * A station is corroborated iff some qualifying decode D has a band matching
 * one of the station's channel bands AND the decode↔station distance (miles)
 * is within `evidenceRadiusMi` of the operator↔decode distance (miles).
 */
export function corroborateStations(
  stations: Station[],
  ring: SlotRecord[],
  opts: EvidenceOptions,
): EvidenceResult {
  const qualifying = collectQualifyingDecodes(ring, opts);
  const sampledBands = [...new Set(qualifying.map((d) => d.band))];

  const corroborated = new Set<string>();
  for (const station of stations) {
    const stationBands = new Set<string>(
      station.channels.map((c) => c.band).filter((b): b is NonNullable<typeof b> => b != null),
    );
    if (stationBands.size === 0) continue;

    for (const d of qualifying) {
      if (!stationBands.has(d.band)) continue;

      const stationDistKm = distanceFromGrids(d.grid, station.grid);
      if (stationDistKm == null) continue;
      const stationDistMi = kmToMi(stationDistKm);

      if (stationDistMi <= evidenceRadiusMi(d.operatorDistMi)) {
        corroborated.add(stationKey(station));
        break; // one qualifying decode is enough; no need to keep scanning
      }
    }
  }

  return { corroborated, sampledBands, considered: stations.length };
}
