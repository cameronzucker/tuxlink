// Pure frequency→band mapping for the Find-a-Station band selector (design §7).
// The selector offers the full amateur HF allocation (160–10 m) — Winlink/VARA/
// ARDOP gateways operate across all of it, so a gateway finder must too. (The
// prior 80/40/30/20 m subset was ALE-channel-shaped, not finder-shaped; the
// voacapl engine accepts the whole 1.8–30 MHz range, so nothing technical
// limited the set.) Everything VHF/UHF is bucketed as line-of-sight 'vhf-uhf'
// (no propagation model, per §10). Dials outside these ranges return null so the
// UI lists them factually without claiming a band colour.

export type Band =
  | '160m'
  | '80m'
  | '60m'
  | '40m'
  | '30m'
  | '20m'
  | '17m'
  | '15m'
  | '12m'
  | '10m'
  | 'vhf-uhf';

/** Selectable HF bands, ascending — drives the band selector order. */
export const HF_BANDS: Band[] = [
  '160m',
  '80m',
  '60m',
  '40m',
  '30m',
  '20m',
  '17m',
  '15m',
  '12m',
  '10m',
];

interface BandRange {
  band: Band;
  loKhz: number;
  hiKhz: number;
}

// Amateur band edges (kHz). HF ranges are the ITU Region 2 amateur allocations;
// 60 m is channelized in the US (five USB channels) — the range spans them. The
// VHF/UHF range is a generous catch-all for 2 m + 70 cm packet dials.
const RANGES: BandRange[] = [
  { band: '160m', loKhz: 1800, hiKhz: 2000 },
  { band: '80m', loKhz: 3500, hiKhz: 4000 },
  { band: '60m', loKhz: 5330.5, hiKhz: 5406.5 },
  { band: '40m', loKhz: 7000, hiKhz: 7300 },
  { band: '30m', loKhz: 10100, hiKhz: 10150 },
  { band: '20m', loKhz: 14000, hiKhz: 14350 },
  { band: '17m', loKhz: 18068, hiKhz: 18168 },
  { band: '15m', loKhz: 21000, hiKhz: 21450 },
  { band: '12m', loKhz: 24890, hiKhz: 24990 },
  { band: '10m', loKhz: 28000, hiKhz: 29700 },
  { band: 'vhf-uhf', loKhz: 50_000, hiKhz: 470_000 },
];

export function bandForKhz(khz: number): Band | null {
  for (const r of RANGES) {
    if (khz >= r.loKhz && khz <= r.hiKhz) return r.band;
  }
  return null;
}

const LABELS: Record<Band, string> = {
  '160m': '160 m',
  '80m': '80 m',
  '60m': '60 m',
  '40m': '40 m',
  '30m': '30 m',
  '20m': '20 m',
  '17m': '17 m',
  '15m': '15 m',
  '12m': '12 m',
  '10m': '10 m',
  'vhf-uhf': 'VHF/UHF',
};

export function bandLabel(band: Band): string {
  return LABELS[band];
}
