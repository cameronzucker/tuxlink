// Pure frequency→band mapping for the Find-a-Station band selector (design §7).
// Only the four HF bands the selector offers (80/40/30/20 m) are modelled for
// reachability; everything VHF/UHF is bucketed as line-of-sight 'vhf-uhf' (no
// propagation model, per §10). Dials outside these ranges return null so the UI
// lists them factually without claiming a band colour.

export type Band = '80m' | '40m' | '30m' | '20m' | 'vhf-uhf';

/** Selectable HF bands, ascending — drives the band selector order. */
export const HF_BANDS: Band[] = ['80m', '40m', '30m', '20m'];

interface BandRange {
  band: Band;
  loKhz: number;
  hiKhz: number;
}

// Amateur band edges (kHz). HF ranges are the ITU Region 2 amateur allocations;
// the VHF/UHF range is a generous catch-all for 2 m + 70 cm packet dials.
const RANGES: BandRange[] = [
  { band: '80m', loKhz: 3500, hiKhz: 4000 },
  { band: '40m', loKhz: 7000, hiKhz: 7300 },
  { band: '30m', loKhz: 10100, hiKhz: 10150 },
  { band: '20m', loKhz: 14000, hiKhz: 14350 },
  { band: 'vhf-uhf', loKhz: 50_000, hiKhz: 470_000 },
];

export function bandForKhz(khz: number): Band | null {
  for (const r of RANGES) {
    if (khz >= r.loKhz && khz <= r.hiKhz) return r.band;
  }
  return null;
}

const LABELS: Record<Band, string> = {
  '80m': '80 m',
  '40m': '40 m',
  '30m': '30 m',
  '20m': '20 m',
  'vhf-uhf': 'VHF/UHF',
};

export function bandLabel(band: Band): string {
  return LABELS[band];
}
