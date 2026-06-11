# Find-a-Station U3 — Map UI (Mock-D surface) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the propagation-aware Find-a-Station map UI (Mock-D surface) that supersedes `CatalogBuilderPanel`, ranks HF stations by predicted reachability (not distance), and hands a chosen channel to the active modem — in a real build.

**Architecture:** A new `StationFinderPanel` (inline overlay, same z-index discipline as the old panel) composed of a top conditions/band/mode bar, a left map (~55%) with reachability-weighted station pins on the offline `BaseMap`, and a right rail (~45%) with selected-station header → antenna-aiming hero → path propagation forecast → channels grouped by frequency with per-channel reliability + `Use →`. Pure logic (band mapping, reachability tiers, station/channel aggregation, prediction binding) lives in small TDD'd modules under `src/catalog/`. The U1 `propagation_predict_path` Tauri command is bound to TS for the first time here; the UI ships distance-ranked and *lights up* when U1 prediction is available, degrading gracefully to "no forecast yet" when `Unavailable`. The #550 operator-location pin is reverted by deleting `CatalogBuilderPanel` (which carried it); `GridPickerOverlay` survives for `GridEdit`.

**Tech Stack:** React 18 + TypeScript, Vitest + @testing-library/react, @tanstack/react-query, Tauri `invoke`, Leaflet/react-leaflet (offline `BaseMap`), existing Maidenhead + haversine utils.

---

## Design references (authoritative)

- Spec: `docs/design/2026-06-10-find-a-station-propagation-map-design.md` §7 (surface), §8 (data model), §11 (approved decisions), §12 (open items).
- Visual: `dev/scratch/2026-06-10-find-a-station-map-mockD-propagation.html` (main checkout; gitignored per-worktree).

## §12 open items — decisions locked for this plan

- **Channel grouping:** by mode → grouped by frequency/band within mode (Mock-D shows "VARA HF" / "ARDOP HF" / "Packet" sections, each frequency once). N0DAJ's shared 7.103 appears once under VARA and once under ARDOP (different modes = different channels), with a "· also ARDOP" sub-note on the VARA row when the same dial carries another mode.
- **Reachability colour thresholds (REL at selected band/current hour):** `good ≥ 0.70`, `fair ≥ 0.40`, `marginal ≥ 0.15`, else `skip` (faded/dashed). Matches Mock-D's good/fair/marginal/unlikely(skip) buckets.
- **Band selector ↔ mode filter interaction:** band selector drives map colouring (one band at a time, default 40 m). Mode chips filter which stations/pins are shown (a station is shown if it has ≥1 channel in an enabled mode). The two are independent: band = colour axis, mode = visibility filter.
- **Recompute cadence:** map reachability recomputes on band change and on station-set change, debounced via React Query caching keyed by `(txGrid, rxGrid, band, utcHour)`; `utcHour` is computed once on open (not a live ticking clock) to avoid jank. A manual "refresh" re-runs.
- **FZ-M1 compact:** at `@media (max-width: 1365px) and (any-pointer: coarse)` collapse side-by-side to map-on-top / rail-below; the propagation panel becomes collapsible.

## File structure

**Create (all under `src/catalog/` unless noted):**

| File | Responsibility |
|---|---|
| `bandPlan.ts` | Pure: frequency kHz → `Band` (`'80m'|'40m'|'30m'|'20m'|'vhf-uhf'|null`); HF-band list; band label. |
| `bandPlan.test.ts` | Unit tests for band mapping. |
| `reachability.ts` | Pure: `ReachTier` (`'good'|'fair'|'marginal'|'skip'`); `relToTier(rel)`; `bestBand(prediction, atHour)`; `tierColorVar(tier)`. |
| `reachability.test.ts` | Unit tests for tiers + best-band. |
| `stationModel.ts` | Pure: `Station`, `Channel`, `baseCallsign(call)`, `aggregateStations(listings)` (collapse SSIDs/modes → pins; expand frequencies → channels; group). |
| `stationModel.test.ts` | Unit tests for aggregation (N0DAJ multi-mode/SSID example from spec §3). |
| `propagationApi.ts` | TS binding for U1: `PathPrediction`, `ChannelReliability` types; `predictPath(txGrid, rxGrid, freqsKhz)`; `isUnavailable(err)`. |
| `propagationApi.test.ts` | Unit tests for the invoke wrapper + Unavailable detection. |
| `useStationPrediction.ts` | React Query hook: selected station + operator grid → `PathPrediction | null` (degrades on Unavailable). |
| `useStationPrediction.test.tsx` | Hook tests with mocked invoke. |
| `useReachabilityMap.ts` | React Query hook: stations + grid + band + hour → `Map<stationKey, ReachTier>` (per-station band prediction; distance-only fallback). |
| `useReachabilityMap.test.tsx` | Hook tests. |
| `StationFinderControls.tsx` | Top bar: conditions readout (UTC/local, SSN provenance, SFI/K degrade), band selector, mode chips, radius, refresh. |
| `StationFinderControls.test.tsx` | Component tests. |
| `StationFinderMap.tsx` | Left ~55%: `BaseMap` + reachability-weighted station pins + me-pin + bearing line + reach legend. |
| `StationFinderMap.test.tsx` | Component tests (leaflet mock). |
| `StationRail.tsx` | Right ~45%: selected-station header, antenna-aiming hero, path propagation forecast, channels-by-frequency with `Use →`. |
| `StationRail.test.tsx` | Component tests. |
| `StationFinderPanel.tsx` | Assembles controls + map + rail; owns selection state; FZ-M1 compact. Replaces `CatalogBuilderPanel`. |
| `StationFinderPanel.css` | Mock-D styling; preserves the `.station-finder-overlay` z-index-above-chrome invariant. |
| `StationFinderPanel.test.tsx` | Panel integration tests. |

**Modify:**

| File | Change |
|---|---|
| `src/shell/AppShell.tsx` | Repoint lazy import + mount to `StationFinderPanel`; widen `catalogPrefillMode` to include `vara-hf`/`vara-fm`; pass operator grid. |
| `src/shell/chrome/menuModel.ts` | Rename label `Find a Gateway…` → `Find a Station…` (keep id `menu:tools:find_gateway`). |
| `src/favorites/types.ts` | (none — `RadioMode` already covers needed modes.) |

**Delete:**

| File | Reason |
|---|---|
| `src/catalog/CatalogBuilderPanel.tsx` | Superseded by `StationFinderPanel`; deletion reverts the #550 pin. |
| `src/catalog/CatalogBuilderPanel.css` | Superseded. |
| `src/catalog/CatalogBuilderPanel.test.tsx` | Superseded. |

**Keep (reused):** `stationTypes.ts`, `useStations.ts`/`useCatalog.ts` (`fetchStations`), `StationResults.tsx`'s `stationFavoriteKey`, `distance.ts`, `src/forms/position/maidenhead.ts`, `src/map/BaseMap.tsx`, `src/favorites/prefillEvent.ts`, `src/favorites/useFavorites.ts`, `src/shell/GridPickerOverlay.tsx` (GridEdit still uses it).

---

### Task 1: Band plan (pure)

**Files:**
- Create: `src/catalog/bandPlan.ts`
- Test: `src/catalog/bandPlan.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/bandPlan.test.ts
import { describe, it, expect } from 'vitest';
import { bandForKhz, HF_BANDS, bandLabel, type Band } from './bandPlan';

describe('bandForKhz', () => {
  it('maps amateur HF dials to their band', () => {
    expect(bandForKhz(3590)).toBe('80m');
    expect(bandForKhz(7103)).toBe('40m');
    expect(bandForKhz(10147)).toBe('30m');
    expect(bandForKhz(14103)).toBe('20m');
  });
  it('maps VHF/UHF packet dials to vhf-uhf', () => {
    expect(bandForKhz(145710)).toBe('vhf-uhf');
    expect(bandForKhz(441300)).toBe('vhf-uhf');
  });
  it('returns null for dials outside the modelled bands', () => {
    expect(bandForKhz(1850)).toBeNull(); // 160m — not in the U3 band selector
    expect(bandForKhz(28120)).toBeNull(); // 10m — not modelled in v1
  });
});

describe('HF_BANDS', () => {
  it('lists the four selectable HF bands in ascending frequency', () => {
    expect(HF_BANDS).toEqual<Band[]>(['80m', '40m', '30m', '20m']);
  });
});

describe('bandLabel', () => {
  it('renders human labels', () => {
    expect(bandLabel('40m')).toBe('40 m');
    expect(bandLabel('vhf-uhf')).toBe('VHF/UHF');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/bandPlan.test.ts`
Expected: FAIL — `bandPlan.ts` does not exist.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/bandPlan.ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/bandPlan.test.ts`
Expected: PASS (all cases).

- [ ] **Step 5: Commit**

```bash
git add src/catalog/bandPlan.ts src/catalog/bandPlan.test.ts
git commit -m "feat(catalog): band-plan mapping for Find-a-Station band selector (tuxlink-gife)"
```

---

### Task 2: Reachability tiers (pure)

**Files:**
- Create: `src/catalog/reachability.ts`
- Test: `src/catalog/reachability.test.ts`
- Depends on: Task 1 (`Band`), Task 3's `PathPrediction` type — but to avoid a cycle, `reachability.ts` imports the prediction types from `propagationApi.ts`. Implement `propagationApi.ts` types first if executing out of order; the types are repeated in Task 3.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/reachability.test.ts
import { describe, it, expect } from 'vitest';
import { relToTier, bestBandNow, tierColorVar, type ReachTier } from './reachability';
import type { PathPrediction } from './propagationApi';

describe('relToTier', () => {
  it('buckets reliability into the four Mock-D tiers', () => {
    expect(relToTier(0.86)).toBe<ReachTier>('good');
    expect(relToTier(0.70)).toBe<ReachTier>('good');
    expect(relToTier(0.58)).toBe<ReachTier>('fair');
    expect(relToTier(0.40)).toBe<ReachTier>('fair');
    expect(relToTier(0.19)).toBe<ReachTier>('marginal');
    expect(relToTier(0.15)).toBe<ReachTier>('marginal');
    expect(relToTier(0.12)).toBe<ReachTier>('skip');
    expect(relToTier(0)).toBe<ReachTier>('skip');
  });
});

describe('tierColorVar', () => {
  it('maps each tier to its CSS custom property', () => {
    expect(tierColorVar('good')).toBe('var(--reach-good)');
    expect(tierColorVar('skip')).toBe('var(--reach-skip)');
  });
});

describe('bestBandNow', () => {
  const prediction: PathPrediction = {
    bearingDeg: 318,
    distanceKm: 77,
    ssn: 118,
    year: 2026,
    month: 6,
    channels: [
      // 80m: rel 0.74 at hour 21; 40m: 0.86; 20m: 0.19
      { frequencyKhz: 3590, voacapMhz: 4, relByHour: hours(0.74), snrByHour: hours(10), mufdayByHour: hours(0.9) },
      { frequencyKhz: 7103, voacapMhz: 7, relByHour: hours(0.86), snrByHour: hours(15), mufdayByHour: hours(1) },
      { frequencyKhz: 14103, voacapMhz: 14, relByHour: hours(0.19), snrByHour: hours(2), mufdayByHour: hours(0.3) },
    ],
  };
  it('returns the band with the highest reliability at the given UTC hour', () => {
    expect(bestBandNow(prediction, 21)).toEqual({ band: '40m', rel: 0.86 });
  });
  it('returns null when no channel maps to a modelled HF band', () => {
    const vhfOnly: PathPrediction = { ...prediction, channels: [
      { frequencyKhz: 145710, voacapMhz: 146, relByHour: hours(0.5), snrByHour: hours(5), mufdayByHour: hours(0.5) },
    ]};
    expect(bestBandNow(vhfOnly, 21)).toBeNull();
  });
});

function hours(v: number): number[] {
  return Array.from({ length: 24 }, () => v);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/reachability.test.ts`
Expected: FAIL — `reachability.ts` does not exist.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/reachability.ts
// Pure reachability bucketing for the Find-a-Station map (design §7, §12).
// REL (VOACAP circuit reliability, 0..1) → one of four tiers driving pin colour
// + size. Thresholds are locked in the plan (§12): good ≥ .70, fair ≥ .40,
// marginal ≥ .15, else skip. The engine is the source of truth for the numbers;
// this only buckets them for display.

import { bandForKhz, type Band } from './bandPlan';
import type { PathPrediction } from './propagationApi';

export type ReachTier = 'good' | 'fair' | 'marginal' | 'skip';

export function relToTier(rel: number): ReachTier {
  if (rel >= 0.70) return 'good';
  if (rel >= 0.40) return 'fair';
  if (rel >= 0.15) return 'marginal';
  return 'skip';
}

const TIER_VAR: Record<ReachTier, string> = {
  good: 'var(--reach-good)',
  fair: 'var(--reach-fair)',
  marginal: 'var(--reach-marginal)',
  skip: 'var(--reach-skip)',
};

export function tierColorVar(tier: ReachTier): string {
  return TIER_VAR[tier];
}

export interface BestBand {
  band: Band;
  rel: number;
}

/** The modelled-HF band with the highest reliability at `utcHour`, or null. */
export function bestBandNow(prediction: PathPrediction, utcHour: number): BestBand | null {
  let best: BestBand | null = null;
  for (const ch of prediction.channels) {
    const band = bandForKhz(ch.frequencyKhz);
    if (!band || band === 'vhf-uhf') continue;
    const rel = ch.relByHour[utcHour] ?? 0;
    if (!best || rel > best.rel) best = { band, rel };
  }
  return best;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/reachability.test.ts`
Expected: PASS. (Requires `propagationApi.ts` to export `PathPrediction`; if executing in order, do Task 3 first or stub the type — the canonical definition is in Task 3.)

- [ ] **Step 5: Commit**

```bash
git add src/catalog/reachability.ts src/catalog/reachability.test.ts
git commit -m "feat(catalog): reachability tier bucketing + best-band-now (tuxlink-gife)"
```

> **Ordering note:** Task 3 defines `PathPrediction`. If running tasks strictly in order, swap Task 2 and Task 3, or create `propagationApi.ts`'s type block first. The plan lists reachability first because it is the smaller pure module; the executor should create `propagationApi.ts` types before compiling `reachability.ts`.

---

### Task 3: Prediction TS binding (U1 `propagation_predict_path`)

**Files:**
- Create: `src/catalog/propagationApi.ts`
- Test: `src/catalog/propagationApi.test.ts`

**Context:** U1 shipped the Rust command `propagation_predict_path` but no TS binding exists. Rust params are `tx_grid`/`rx_grid`/`frequencies_khz`; **Tauri v2 auto-converts camelCase JS args → snake_case Rust params** (verified: working code passes `tsLocal` for Rust `ts_local`), so the invoke passes `{ txGrid, rxGrid, frequenciesKhz }`. The response is camelCase serde (`#[serde(rename_all = "camelCase")]`, verified). When the engine is not bundled the command throws `UiError::Unavailable { reason }`, which arrives as a thrown value `{ kind: 'Unavailable', reason: string }` (matches `catalogErrorMessage` handling of UiError variants). Max 11 frequencies per call; HF only (1.8–28 MHz) — out-of-range dials are rejected by the backend, so the caller filters to HF before invoking.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/propagationApi.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { predictPath, isUnavailable, type PathPrediction } from './propagationApi';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('predictPath', () => {
  it('invokes propagation_predict_path with snake_case args and returns the prediction', async () => {
    const resp: PathPrediction = {
      bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
      channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.8), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
    };
    vi.mocked(invoke).mockResolvedValue(resp as unknown as never);
    const got = await predictPath('DM43bp', 'DM34oa', [7103, 14103]);
    expect(invoke).toHaveBeenCalledWith('propagation_predict_path', {
      txGrid: 'DM43bp', rxGrid: 'DM34oa', frequenciesKhz: [7103, 14103],
    });
    expect(got).toEqual(resp);
  });

  it('caps at 11 frequencies (backend rejects more)', async () => {
    vi.mocked(invoke).mockResolvedValue({ channels: [] } as unknown as never);
    const many = Array.from({ length: 20 }, (_, i) => 7000 + i);
    await predictPath('DM43bp', 'DM34oa', many);
    const arg = vi.mocked(invoke).mock.calls[0][1] as { frequenciesKhz: number[] };
    expect(arg.frequenciesKhz).toHaveLength(11);
  });
});

describe('isUnavailable', () => {
  it('recognises the UiError::Unavailable variant', () => {
    expect(isUnavailable({ kind: 'Unavailable', reason: 'voacapl not bundled' })).toBe(true);
    expect(isUnavailable({ kind: 'Rejected', reason: 'bad grid' })).toBe(false);
    expect(isUnavailable(new Error('boom'))).toBe(false);
    expect(isUnavailable(null)).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/propagationApi.test.ts`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/propagationApi.ts
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
): Promise<PathPrediction> {
  // Tauri v2 maps these camelCase keys to the Rust snake_case params.
  return invoke<PathPrediction>('propagation_predict_path', {
    txGrid,
    rxGrid,
    frequenciesKhz: frequenciesKhz.slice(0, MAX_FREQUENCIES),
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/propagationApi.test.ts`
Expected: PASS.

- [ ] **Step 5: Verify the Rust arg/response shape matches (grounding)**

Run: `rg -n 'pub fn propagation_predict_path|tx_grid|rx_grid|frequencies_khz|pub struct PathPrediction|pub struct ChannelReliability|bearingDeg|bearing_deg' src-tauri/src/propagation`
Expected: command signature uses `tx_grid`/`rx_grid`/`frequencies_khz`; structs carry `bearing_deg`, `distance_km`, `ssn`, `year`, `month`, `channels` with `frequency_khz`/`voacap_mhz`/`rel_by_hour`/`snr_by_hour`/`mufday_by_hour` under a camelCase serde rename. **If field names differ, fix `propagationApi.ts` to match the Rust serde and re-run Step 4.**

- [ ] **Step 6: Commit**

```bash
git add src/catalog/propagationApi.ts src/catalog/propagationApi.test.ts
git commit -m "feat(catalog): TS binding for U1 propagation_predict_path (tuxlink-gife)"
```

---

### Task 4: Station / channel aggregation model (pure)

**Files:**
- Create: `src/catalog/stationModel.ts`
- Test: `src/catalog/stationModel.test.ts`
- Reuses: `Gateway`, `StationListing`, `ListingMode` from `stationTypes.ts`; `bandForKhz` from `bandPlan.ts`.

**Context (spec §8):** A **Station** aggregates by `(base callsign, grid)` — N0DAJ and N0DAJ-10/-11/-12 collapse to one pin. A **Channel** = `(mode, frequencyKhz, ssid?)`; SSID is set for packet (the connect target). Each mode-listing's `frequenciesKhz[]` expands into one channel per frequency. The same dial under two modes is two channels.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/stationModel.test.ts
import { describe, it, expect } from 'vitest';
import { baseCallsign, aggregateStations, type Station } from './stationModel';
import type { Gateway, StationListing } from './stationTypes';

function gw(partial: Partial<Gateway> & { callsign: string }): Gateway {
  return {
    channel: partial.callsign, callsign: partial.callsign,
    sysopName: partial.sysopName ?? null, grid: partial.grid ?? 'DM34oa',
    location: partial.location ?? null, frequenciesKhz: partial.frequenciesKhz ?? [],
    lastUpdate: partial.lastUpdate ?? null, email: null, homepage: null,
  };
}

describe('baseCallsign', () => {
  it('strips an SSID suffix', () => {
    expect(baseCallsign('N0DAJ-10')).toBe('N0DAJ');
    expect(baseCallsign('N0DAJ')).toBe('N0DAJ');
    expect(baseCallsign('w7ara-1')).toBe('W7ARA');
  });
});

describe('aggregateStations — N0DAJ multi-mode/SSID (spec §3)', () => {
  const listings: StationListing[] = [
    { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug Jarmuth', location: 'Wickenburg, AZ',
        frequenciesKhz: [3590, 7103, 7108, 10147, 14103, 14115] })] },
    { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [3590, 7103, 7108, 14103, 14115] })] },
    { mode: 'packet', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [
        gw({ callsign: 'N0DAJ-10', grid: 'DM34oa', frequenciesKhz: [145710] }),
        gw({ callsign: 'N0DAJ-11', grid: 'DM34oa', frequenciesKhz: [145010] }),
        gw({ callsign: 'N0DAJ-12', grid: 'DM34oa', frequenciesKhz: [441300] }),
      ] },
  ];

  it('collapses all listings into one station pin keyed by base call + grid', () => {
    const stations = aggregateStations(listings);
    expect(stations).toHaveLength(1);
    const s = stations[0];
    expect(s.baseCallsign).toBe('N0DAJ');
    expect(s.grid).toBe('DM34oa');
    expect(s.sysopName).toBe('Doug Jarmuth');
    expect(s.location).toBe('Wickenburg, AZ');
    expect(s.modes.sort()).toEqual(['ardop-hf', 'packet', 'vara-hf']);
  });

  it('expands each mode-listing frequency into a channel; shared dial under two modes = two channels', () => {
    const s = aggregateStations(listings)[0];
    const vara7103 = s.channels.filter((c) => c.mode === 'vara-hf' && c.frequencyKhz === 7103);
    const ardop7103 = s.channels.filter((c) => c.mode === 'ardop-hf' && c.frequencyKhz === 7103);
    expect(vara7103).toHaveLength(1);
    expect(ardop7103).toHaveLength(1);
    expect(s.channels.filter((c) => c.mode === 'vara-hf')).toHaveLength(6);
  });

  it('carries the SSID as the packet connect target', () => {
    const s = aggregateStations(listings)[0];
    const pkt = s.channels.filter((c) => c.mode === 'packet');
    expect(pkt.map((c) => c.ssid).sort()).toEqual(['N0DAJ-10', 'N0DAJ-11', 'N0DAJ-12']);
    expect(pkt.find((c) => c.frequencyKhz === 145710)?.ssid).toBe('N0DAJ-10');
  });

  it('tags each channel with its band', () => {
    const s = aggregateStations(listings)[0];
    expect(s.channels.find((c) => c.mode === 'vara-hf' && c.frequencyKhz === 7103)?.band).toBe('40m');
    expect(s.channels.find((c) => c.mode === 'packet' && c.frequencyKhz === 145710)?.band).toBe('vhf-uhf');
  });
});

describe('aggregateStations — distinct stations', () => {
  it('keeps stations with different base calls separate', () => {
    const listings: StationListing[] = [
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103] }),
                   gw({ callsign: 'K7UAZ', grid: 'DM43aa', frequenciesKhz: [7103] })] },
    ];
    const stations = aggregateStations(listings);
    expect(stations.map((s: Station) => s.baseCallsign).sort()).toEqual(['K7UAZ', 'N0DAJ']);
  });
  it('drops gateways with no grid (cannot place on map)', () => {
    const listings: StationListing[] = [
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'NOGRID', grid: null, frequenciesKhz: [7103] })] },
    ];
    expect(aggregateStations(listings)).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/stationModel.test.ts`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/stationModel.ts
// Station/channel aggregation for Find-a-Station (design §8).
// Collapses the per-mode StationListing[] (one row per callsign+SSID per mode)
// into Station pins keyed by (base callsign, grid), each carrying the expanded
// set of Channels = (mode, frequencyKhz, ssid?, band). One pin per location;
// one channel per (mode, dial). SSID is the packet connect target; HF channels
// share the base call.

import { bandForKhz, type Band } from './bandPlan';
import type { Gateway, ListingMode, StationListing } from './stationTypes';

export interface Channel {
  mode: ListingMode;
  frequencyKhz: number;
  /** Packet connect target (e.g. N0DAJ-10); undefined for HF (base call dials). */
  ssid?: string;
  band: Band | null;
}

export interface Station {
  /** Aggregation key part 1 — SSID-stripped, upper-cased call. */
  baseCallsign: string;
  /** Aggregation key part 2 — Maidenhead grid (non-null; gridless rows dropped). */
  grid: string;
  sysopName: string | null;
  location: string | null;
  /** Distinct modes this station offers, for the pin's mode badges. */
  modes: ListingMode[];
  channels: Channel[];
  /** Most-recent fetch stamp across contributing listings (freshness caption). */
  fetchedAtMs: number | null;
}

/** Strip a trailing -NN SSID and upper-case. */
export function baseCallsign(call: string): string {
  return call.trim().toUpperCase().replace(/-\d+$/, '');
}

function hasSsid(call: string): boolean {
  return /-\d+$/.test(call.trim());
}

export function aggregateStations(listings: StationListing[]): Station[] {
  const byKey = new Map<string, Station>();

  for (const listing of listings) {
    for (const g of listing.gateways) {
      const grid = g.grid?.trim();
      if (!grid) continue; // no grid → unplaceable on the map (spec: map needs lat/lon)
      const base = baseCallsign(g.callsign);
      const key = `${base}|${grid.toUpperCase()}`;

      let station = byKey.get(key);
      if (!station) {
        station = {
          baseCallsign: base, grid, sysopName: g.sysopName, location: g.location,
          modes: [], channels: [], fetchedAtMs: listing.fetchedAtMs,
        };
        byKey.set(key, station);
      }
      // Fill in identity metadata from whichever listing first carries it.
      if (!station.sysopName && g.sysopName) station.sysopName = g.sysopName;
      if (!station.location && g.location) station.location = g.location;
      if (listing.fetchedAtMs && (!station.fetchedAtMs || listing.fetchedAtMs > station.fetchedAtMs)) {
        station.fetchedAtMs = listing.fetchedAtMs;
      }
      if (!station.modes.includes(listing.mode)) station.modes.push(listing.mode);

      const ssid = hasSsid(g.callsign) ? g.callsign.trim().toUpperCase() : undefined;
      for (const khz of g.frequenciesKhz) {
        station.channels.push({ mode: listing.mode, frequencyKhz: khz, ssid, band: bandForKhz(khz) });
      }
    }
  }

  return [...byKey.values()];
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/stationModel.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/stationModel.ts src/catalog/stationModel.test.ts
git commit -m "feat(catalog): station/channel aggregation model (tuxlink-gife)"
```

---

### Task 5: Selected-station prediction hook

**Files:**
- Create: `src/catalog/useStationPrediction.ts`
- Test: `src/catalog/useStationPrediction.test.tsx`
- Reuses: `predictPath`, `isUnavailable`, `PathPrediction` (Task 3); `Station` (Task 4); `@tanstack/react-query`.

**Context:** Given the operator grid + a selected `Station`, predict over the station's distinct HF channel frequencies (≤11) once and cache. Returns `{ prediction, status }` where status is `'ok' | 'unavailable' | 'no-location' | 'loading' | 'error'`. Distance-only fallback is the consumer's job; this hook only reports availability.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/useStationPrediction.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useStationPrediction } from './useStationPrediction';
import type { Station } from './stationModel';

const station: Station = {
  baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug', location: 'Wickenburg, AZ',
  modes: ['vara-hf'], fetchedAtMs: 1,
  channels: [
    { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
    { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
  ],
};

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useStationPrediction', () => {
  it('predicts over the station HF dials (deduped, VHF excluded) and returns ok', async () => {
    vi.mocked(invoke).mockResolvedValue({
      bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
      channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.8), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
    } as unknown as never);
    const { result } = renderHook(() => useStationPrediction('DM43bp', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('ok'));
    const arg = vi.mocked(invoke).mock.calls[0][1] as { frequenciesKhz: number[] };
    expect(arg.frequenciesKhz.slice().sort((a, b) => a - b)).toEqual([3590, 7103]); // VHF dropped
    expect(result.current.prediction?.bearingDeg).toBe(318);
  });

  it('reports unavailable (not error) when the engine is not bundled', async () => {
    vi.mocked(invoke).mockRejectedValue({ kind: 'Unavailable', reason: 'voacapl not bundled' });
    const { result } = renderHook(() => useStationPrediction('DM43bp', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('unavailable'));
    expect(result.current.prediction).toBeNull();
  });

  it('reports no-location when the operator grid is empty', async () => {
    const { result } = renderHook(() => useStationPrediction('', station), { wrapper: wrap() });
    await waitFor(() => expect(result.current.status).toBe('no-location'));
    expect(invoke).not.toHaveBeenCalled();
  });

  it('is idle with no station selected', async () => {
    const { result } = renderHook(() => useStationPrediction('DM43bp', null), { wrapper: wrap() });
    expect(result.current.status).toBe('idle');
    expect(invoke).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/useStationPrediction.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/useStationPrediction.ts
// React Query hook: predict the selected station's HF path once and cache it.
// Returns a discriminated status so the UI can light up the propagation panel
// when 'ok', show "set your location" when 'no-location', and silently fall back
// to distance-only when 'unavailable' (engine not bundled — the U1 degrade path).

import { useQuery } from '@tanstack/react-query';
import { predictPath, isUnavailable, type PathPrediction } from './propagationApi';
import type { Station } from './stationModel';

export type PredictionStatus = 'idle' | 'no-location' | 'loading' | 'ok' | 'unavailable' | 'error';

export interface StationPredictionResult {
  prediction: PathPrediction | null;
  status: PredictionStatus;
}

/** Distinct HF dials for a station, ascending, capped to the engine's 11. */
export function hfDials(station: Station): number[] {
  const set = new Set<number>();
  for (const ch of station.channels) {
    if (ch.band && ch.band !== 'vhf-uhf') set.add(ch.frequencyKhz);
  }
  return [...set].sort((a, b) => a - b).slice(0, 11);
}

export function useStationPrediction(
  operatorGrid: string,
  station: Station | null,
): StationPredictionResult {
  const grid = operatorGrid.trim();
  const dials = station ? hfDials(station) : [];
  const enabled = Boolean(station) && grid.length > 0 && dials.length > 0;

  const query = useQuery({
    queryKey: ['propagation', grid, station?.baseCallsign, station?.grid, dials],
    enabled,
    staleTime: 5 * 60_000,
    retry: false,
    queryFn: async () => {
      try {
        return await predictPath(grid, station!.grid, dials);
      } catch (err) {
        if (isUnavailable(err)) return 'unavailable' as const;
        throw err;
      }
    },
  });

  if (!station) return { prediction: null, status: 'idle' };
  if (grid.length === 0) return { prediction: null, status: 'no-location' };
  if (query.isLoading || query.isFetching) return { prediction: null, status: 'loading' };
  if (query.isError) return { prediction: null, status: 'error' };
  if (query.data === 'unavailable') return { prediction: null, status: 'unavailable' };
  if (query.data) return { prediction: query.data, status: 'ok' };
  return { prediction: null, status: 'loading' };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/useStationPrediction.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/useStationPrediction.ts src/catalog/useStationPrediction.test.tsx
git commit -m "feat(catalog): selected-station prediction hook with degrade status (tuxlink-gife)"
```

---

### Task 6: Reachability-map hook (per-station band ranking)

**Files:**
- Create: `src/catalog/useReachabilityMap.ts`
- Test: `src/catalog/useReachabilityMap.test.tsx`
- Reuses: `predictPath`/`isUnavailable` (Task 3), `Station` (Task 4), `relToTier`/`ReachTier` (Task 2), `bandForKhz` (Task 1), `distanceFromGrids`/`kmToMi` (`distance.ts`).

**Context:** For the selected band + UTC hour, compute a `ReachTier` for each station that has a channel on that band, by predicting that station's representative band dial and reading current-hour REL. Returns `{ tiers: Map<stationKey, ReachTier>, distances: Map<stationKey, number>, available }`. When the engine is `Unavailable`, `available=false` and `tiers` is empty (consumer ranks by distance). `stationKey(station)` = `${baseCallsign}|${grid}`.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/useReachabilityMap.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useReachabilityMap, stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

function station(call: string, grid: string, khz: number[]): Station {
  return { baseCallsign: call, grid, sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1,
    channels: khz.map((f) => ({ mode: 'vara-hf' as const, frequencyKhz: f, band: f < 8000 ? '40m' as const : '20m' as const })) };
}

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useReachabilityMap', () => {
  const stations = [station('N0DAJ', 'DM34oa', [7103]), station('K0ABC', 'EN34', [7103])];

  it('assigns a tier per station from current-hour REL on the selected band', async () => {
    vi.mocked(invoke).mockImplementation(async (_cmd, args) => {
      const rx = (args as { rxGrid: string }).rxGrid;
      const rel = rx === 'DM34oa' ? 0.86 : 0.12; // near=good, far=skip on 40m
      return { bearingDeg: 0, distanceKm: 1, ssn: 118, year: 2026, month: 6,
        channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(rel), snrByHour: Array(24).fill(5), mufdayByHour: Array(24).fill(0.5) }] } as unknown as never;
    });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.available).toBe(true));
    expect(result.current.tiers.get(stationKey(stations[0]))).toBe('good');
    expect(result.current.tiers.get(stationKey(stations[1]))).toBe('skip');
  });

  it('marks unavailable + empty tiers when the engine is not bundled', async () => {
    vi.mocked(invoke).mockRejectedValue({ kind: 'Unavailable', reason: 'no voacapl' });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.available).toBe(false);
    expect(result.current.tiers.size).toBe(0);
  });

  it('always provides distances regardless of prediction availability', async () => {
    vi.mocked(invoke).mockRejectedValue({ kind: 'Unavailable' });
    const { result } = renderHook(() => useReachabilityMap('DM43bp', stations, '40m', 21), { wrapper: wrap() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.distances.get(stationKey(stations[0]))).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/useReachabilityMap.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/useReachabilityMap.ts
// React Query hook: rank visible stations by predicted reachability on the
// selected band at a fixed UTC hour (design §7 — "distance is not the ranking;
// reachability is"). One point-to-point voacapl run per station on a
// representative band dial (§5 option a). Degrades to distance-only when the
// engine is Unavailable: tiers empty, available=false, distances always present.

import { useQuery } from '@tanstack/react-query';
import { predictPath, isUnavailable } from './propagationApi';
import { relToTier, type ReachTier } from './reachability';
import { distanceFromGrids, kmToMi } from './distance';
import type { Band } from './bandPlan';
import type { Station } from './stationModel';

export function stationKey(s: Station): string {
  return `${s.baseCallsign}|${s.grid}`;
}

export interface ReachabilityMap {
  tiers: Map<string, ReachTier>;
  distances: Map<string, number>; // miles from operator grid
  available: boolean;
  loading: boolean;
}

/** First channel frequency a station offers on `band`, or null. */
function bandDial(station: Station, band: Band): number | null {
  const ch = station.channels.find((c) => c.band === band);
  return ch ? ch.frequencyKhz : null;
}

export function useReachabilityMap(
  operatorGrid: string,
  stations: Station[],
  band: Band,
  utcHour: number,
): ReachabilityMap {
  const grid = operatorGrid.trim();
  const keys = stations.map(stationKey).join(',');

  // Distances are pure + always available.
  const distances = new Map<string, number>();
  for (const s of stations) {
    const km = grid ? distanceFromGrids(grid, s.grid) : null;
    if (km != null) distances.set(stationKey(s), kmToMi(km));
  }

  const onBand = stations.filter((s) => bandDial(s, band) != null);
  const enabled = grid.length > 0 && band !== 'vhf-uhf' && onBand.length > 0;

  const query = useQuery({
    queryKey: ['reachability', grid, band, utcHour, keys],
    enabled,
    staleTime: 5 * 60_000,
    retry: false,
    queryFn: async () => {
      const tiers = new Map<string, ReachTier>();
      let sawUnavailable = false;
      for (const s of onBand) {
        const dial = bandDial(s, band)!;
        try {
          const p = await predictPath(grid, s.grid, [dial]);
          const rel = p.channels[0]?.relByHour[utcHour] ?? 0;
          tiers.set(stationKey(s), relToTier(rel));
        } catch (err) {
          if (isUnavailable(err)) { sawUnavailable = true; break; }
          // A single-station failure is non-fatal: leave it untiered.
        }
      }
      if (sawUnavailable) return { tiers: new Map<string, ReachTier>(), available: false };
      return { tiers, available: true };
    },
  });

  return {
    tiers: query.data?.tiers ?? new Map(),
    distances,
    available: query.data?.available ?? false,
    loading: enabled && (query.isLoading || query.isFetching),
  };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/useReachabilityMap.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/useReachabilityMap.ts src/catalog/useReachabilityMap.test.tsx
git commit -m "feat(catalog): reachability-map hook (per-station band ranking + distance fallback) (tuxlink-gife)"
```

---

### Task 7: Channel grouping + Use→ dial builder (pure)

**Files:**
- Create: `src/catalog/channelGrouping.ts`
- Test: `src/catalog/channelGrouping.test.ts`
- Reuses: `Channel`, `Station` (Task 4); `FavoriteDial`, `RadioMode` (`../favorites/types`); `PathPrediction` (Task 3); `relToTier` (Task 2).

**Context (spec §7):** Right-rail channels are grouped by mode, each frequency shown once with the mode(s) on it; per-channel reliability pip from the prediction; `Use →` builds a `FavoriteDial` for exactly that mode+freq(+ssid). `ListingMode` → `RadioMode` mapping: `vara-hf`→`vara-hf`, `ardop-hf`→`ardop-hf`, `packet`→`packet` (pactor/robust-packet have no modem → not prefillable, `null`).

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/channelGrouping.test.ts
import { describe, it, expect } from 'vitest';
import { groupChannelsByMode, channelToDial, channelReliability, type ChannelGroup } from './channelGrouping';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';

const channels: Channel[] = [
  { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
  { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
  { mode: 'ardop-hf', frequencyKhz: 7103, band: '40m' },
  { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
];
const station: Station = { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null,
  modes: ['vara-hf', 'ardop-hf', 'packet'], channels, fetchedAtMs: 1 };

describe('groupChannelsByMode', () => {
  it('groups channels under their mode, ascending by frequency', () => {
    const groups = groupChannelsByMode(station);
    const vara = groups.find((g: ChannelGroup) => g.mode === 'vara-hf')!;
    expect(vara.channels.map((c) => c.frequencyKhz)).toEqual([3590, 7103]);
    expect(groups.map((g) => g.mode)).toEqual(['vara-hf', 'ardop-hf', 'packet']);
  });
});

describe('channelToDial', () => {
  it('builds an HF dial keyed on the base call', () => {
    expect(channelToDial(station, channels[0])).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
  });
  it('uses the SSID as the packet connect target', () => {
    expect(channelToDial(station, channels[3])).toEqual({ mode: 'packet', gateway: 'N0DAJ-10', freq: '145.710', grid: 'DM34oa' });
  });
  it('returns null for a non-prefillable mode', () => {
    const pactor: Channel = { mode: 'pactor', frequencyKhz: 7103, band: '40m' };
    expect(channelToDial(station, pactor)).toBeNull();
  });
});

describe('channelReliability', () => {
  const prediction: PathPrediction = { bearingDeg: 0, distanceKm: 1, ssn: 118, year: 2026, month: 6,
    channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }] };
  it('returns rel + tier for an HF channel present in the prediction', () => {
    expect(channelReliability(channels[0], prediction, 21)).toEqual({ rel: 0.86, tier: 'good' });
  });
  it('returns null for a VHF/UHF channel (no model)', () => {
    expect(channelReliability(channels[3], prediction, 21)).toBeNull();
  });
  it('returns null when the prediction lacks that dial', () => {
    expect(channelReliability(channels[1], prediction, 21)).toBeNull(); // 3590 not in prediction
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/channelGrouping.test.ts`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/channelGrouping.ts
// Right-rail channel presentation (design §7): group a station's channels by
// mode (each frequency once, ascending), attach per-channel reliability from the
// path prediction, and build the FavoriteDial handed to the modem on Use →.

import type { Channel, Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import { relToTier, type ReachTier } from './reachability';
import type { FavoriteDial, RadioMode } from '../favorites/types';
import type { ListingMode } from './stationTypes';

export interface ChannelGroup {
  mode: ListingMode;
  channels: Channel[];
}

const MODE_ORDER: ListingMode[] = ['vara-hf', 'ardop-hf', 'packet', 'pactor', 'robust-packet'];

export function groupChannelsByMode(station: Station): ChannelGroup[] {
  const byMode = new Map<ListingMode, Channel[]>();
  for (const ch of station.channels) {
    const list = byMode.get(ch.mode) ?? [];
    list.push(ch);
    byMode.set(ch.mode, list);
  }
  return MODE_ORDER.filter((m) => byMode.has(m)).map((mode) => ({
    mode,
    channels: byMode.get(mode)!.slice().sort((a, b) => a.frequencyKhz - b.frequencyKhz),
  }));
}

/** ListingMode → modem RadioMode; null for modes with no prefillable modem. */
function radioModeFor(mode: ListingMode): RadioMode | null {
  if (mode === 'vara-hf' || mode === 'ardop-hf' || mode === 'packet') return mode;
  return null;
}

const mhz = (khz: number): string => (khz / 1000).toFixed(3);

export function channelToDial(station: Station, channel: Channel): FavoriteDial | null {
  const mode = radioModeFor(channel.mode);
  if (!mode) return null;
  return {
    mode,
    gateway: channel.ssid ?? station.baseCallsign,
    freq: mhz(channel.frequencyKhz),
    grid: station.grid,
  };
}

export interface ChannelReliabilityResult {
  rel: number;
  tier: ReachTier;
}

export function channelReliability(
  channel: Channel,
  prediction: PathPrediction,
  utcHour: number,
): ChannelReliabilityResult | null {
  if (channel.band === 'vhf-uhf' || channel.band == null) return null;
  const pc = prediction.channels.find((c) => c.frequencyKhz === channel.frequencyKhz);
  if (!pc) return null;
  const rel = pc.relByHour[utcHour] ?? 0;
  return { rel, tier: relToTier(rel) };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/channelGrouping.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/channelGrouping.ts src/catalog/channelGrouping.test.ts
git commit -m "feat(catalog): channel grouping + Use-dial builder + per-channel reliability (tuxlink-gife)"
```

---

### Task 8: Conditions / band / mode controls bar

**Files:**
- Create: `src/catalog/StationFinderControls.tsx`
- Test: `src/catalog/StationFinderControls.test.tsx`
- Reuses: `HF_BANDS`/`bandLabel`/`Band` (Task 1); `LISTING_MODES`/`ListingMode` (`stationTypes.ts`).

**Context (spec §7 top bar):** conditions readout (UTC + local time, SSN provenance "solar data N old"; SFI/K-index shown only if present — degrade gracefully, do not fabricate), a band selector (80/40/30/20 m, default 40 m, plus a disabled VHF/UHF affordance), mode chips (VARA/ARDOP/Packet), and a refresh control. The component is presentational — parent owns state.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/StationFinderControls.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StationFinderControls } from './StationFinderControls';

const baseProps = {
  band: '40m' as const, onBandChange: vi.fn(),
  enabledModes: new Set<'vara-hf' | 'ardop-hf' | 'packet'>(['vara-hf', 'ardop-hf', 'packet']),
  onToggleMode: vi.fn(),
  utcHour: 21, localTime: '14:20', ssn: 118, ssnAgeDays: 2,
  predictionAvailable: true, onRefresh: vi.fn(), refreshing: false,
};

describe('StationFinderControls', () => {
  it('renders the four HF bands and marks the selected one', () => {
    render(<StationFinderControls {...baseProps} />);
    expect(screen.getByRole('button', { name: /80 m/ })).toBeTruthy();
    expect(screen.getByRole('button', { name: /40 m/ }).getAttribute('aria-pressed')).toBe('true');
  });

  it('fires onBandChange when another band is clicked', () => {
    const onBandChange = vi.fn();
    render(<StationFinderControls {...baseProps} onBandChange={onBandChange} />);
    fireEvent.click(screen.getByRole('button', { name: /20 m/ }));
    expect(onBandChange).toHaveBeenCalledWith('20m');
  });

  it('shows SSN provenance and degrades SFI/K when absent', () => {
    render(<StationFinderControls {...baseProps} />);
    expect(screen.getByText(/SSN 118/)).toBeTruthy();
    expect(screen.getByText(/solar data 2d old/)).toBeTruthy();
    // SFI/K not provided → not rendered as fabricated values
    expect(screen.queryByText(/SFI/)).toBeNull();
  });

  it('toggles a mode chip', () => {
    const onToggleMode = vi.fn();
    render(<StationFinderControls {...baseProps} onToggleMode={onToggleMode} />);
    fireEvent.click(screen.getByRole('button', { name: /VARA HF/ }));
    expect(onToggleMode).toHaveBeenCalledWith('vara-hf');
  });

  it('notes when prediction is unavailable (distance-only)', () => {
    render(<StationFinderControls {...baseProps} predictionAvailable={false} />);
    expect(screen.getByText(/no forecast/i)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/StationFinderControls.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/StationFinderControls.tsx
// Top conditions/band/mode bar for Find-a-Station (design §7). Presentational:
// the parent owns band + mode-filter state. SSN provenance is shown from the
// prediction (F12); SFI/K-index are shown only when a value is supplied — never
// fabricated (amateur-radio-reliability discipline: display only what we have).

import { HF_BANDS, bandLabel, type Band } from './bandPlan';

export type FilterMode = 'vara-hf' | 'ardop-hf' | 'packet';

const FILTER_MODES: { mode: FilterMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'packet', label: 'Packet' },
];

export interface StationFinderControlsProps {
  band: Band;
  onBandChange: (band: Band) => void;
  enabledModes: Set<FilterMode>;
  onToggleMode: (mode: FilterMode) => void;
  utcHour: number;
  localTime: string;
  ssn: number | null;
  ssnAgeDays: number | null;
  sfi?: number | null;
  kIndex?: number | null;
  predictionAvailable: boolean;
  onRefresh: () => void;
  refreshing: boolean;
}

export function StationFinderControls(props: StationFinderControlsProps) {
  const utcLabel = `${String(props.utcHour).padStart(2, '0')}:00Z`;
  return (
    <div className="station-finder__controls">
      <div className="station-finder__cond" data-testid="conditions">
        <span>{props.localTime} local · <b>{utcLabel}</b></span>
        {props.sfi != null && <span>SFI <b>{props.sfi}</b></span>}
        {props.ssn != null && <span>SSN <b>{props.ssn}</b></span>}
        {props.kIndex != null && <span>K <b>{props.kIndex}</b></span>}
        {props.ssnAgeDays != null && (
          <span className="station-finder__stale">solar data {props.ssnAgeDays}d old</span>
        )}
        {!props.predictionAvailable && (
          <span className="station-finder__stale">no forecast — distance only</span>
        )}
      </div>

      <div className="station-finder__bandbar">
        <span className="station-finder__lab">Reachability on</span>
        {HF_BANDS.map((b) => (
          <button
            key={b}
            type="button"
            className={`station-finder__bandtab${props.band === b ? ' on' : ''}`}
            aria-pressed={props.band === b}
            onClick={() => props.onBandChange(b)}
          >
            {bandLabel(b)}
          </button>
        ))}
        <button type="button" className="station-finder__bandtab" disabled aria-disabled title="No propagation model for VHF/UHF">
          VHF/UHF
        </button>

        <span className="station-finder__modes">
          {FILTER_MODES.map(({ mode, label }) => (
            <button
              key={mode}
              type="button"
              className={`station-finder__chip${props.enabledModes.has(mode) ? ' on' : ' off'}`}
              aria-pressed={props.enabledModes.has(mode)}
              onClick={() => props.onToggleMode(mode)}
            >
              <span className={`station-finder__sw station-finder__sw--${mode}`} />
              {label}
            </button>
          ))}
        </span>

        <button type="button" className="station-finder__refresh" onClick={props.onRefresh} disabled={props.refreshing}>
          {props.refreshing ? 'Checking…' : 'Check for newer list'}
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/StationFinderControls.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/StationFinderControls.tsx src/catalog/StationFinderControls.test.tsx
git commit -m "feat(catalog): Find-a-Station conditions/band/mode controls bar (tuxlink-gife)"
```

---

### Task 9: Station map (left pane)

**Files:**
- Create: `src/catalog/StationFinderMap.tsx`
- Test: `src/catalog/StationFinderMap.test.tsx`
- Reuses: `BaseMap` (`../map/BaseMap`), `gridToLatLon` (`../forms/position/maidenhead`), `Station`/`stationKey` (Tasks 4/6), `ReachTier`/`tierColorVar` (Task 2). Tests use the leaflet mock (`../map/testMapMock`) exactly as `CatalogBuilderPanel.test.tsx` did.

**Context (spec §7 map):** one pin per station at its grid lat/lon; HF pins coloured/sized by their `ReachTier` for the selected band; a "you" pin at the operator grid; clicking a pin selects the station. Distance rings demoted to backdrop. When prediction unavailable, pins render in a neutral "untiered" style and the operator still selects by clicking. Pin screen placement reuses `BaseMap`'s child-marker mechanism — render `<Marker>` children positioned via lat/lon (the same pattern `GridMapPicker` uses). Because `BaseMap`'s contract is frozen (C11), do NOT widen its props; place pins as children.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/StationFinderMap.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
import { resetMapMock } from '../map/testMapMock';

import { StationFinderMap } from './StationFinderMap';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

const stations: Station[] = [
  { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'K0ABC', grid: 'EN34', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
];
const tiers = new Map([[stationKey(stations[0]), 'good' as const], [stationKey(stations[1]), 'skip' as const]]);

beforeEach(() => resetMapMock());

describe('StationFinderMap', () => {
  it('renders a pin per station with a reach-tier class', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={() => {}} />);
    const pins = screen.getAllByTestId('station-pin');
    expect(pins).toHaveLength(2);
    expect(pins[0].className).toMatch(/good/);
    expect(pins[1].className).toMatch(/skip/);
  });

  it('selects a station when its pin is clicked', () => {
    const onSelect = vi.fn();
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={onSelect} />);
    fireEvent.click(screen.getAllByTestId('station-pin')[0]);
    expect(onSelect).toHaveBeenCalledWith(stations[0]);
  });

  it('renders an untiered pin when no tier is known (prediction unavailable)', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    const pins = screen.getAllByTestId('station-pin');
    expect(pins[0].className).toMatch(/untiered/);
  });

  it('renders the operator "you" pin', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={tiers} selectedKey={null} onSelect={() => {}} />);
    expect(screen.getByTestId('me-pin')).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/StationFinderMap.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Inspect the BaseMap child-marker pattern, then implement**

First, read how `GridMapPicker` places a child `<Marker>` so the new pins match the project's leaflet usage:

Run: `rg -n 'Marker|position=|useMap|divIcon|L\.' src/map/GridMapPicker.tsx`

Then implement (adapt marker/icon usage to match what GridMapPicker + the mock expose — the mock renders `<Marker>` children; we wrap each in a clickable element carrying `data-testid="station-pin"`):

```typescript
// src/catalog/StationFinderMap.tsx
// Left-pane station map (design §7). One pin per station at its grid centroid,
// coloured/sized by its reachability tier on the selected band; an operator
// "you" pin; click-to-select. Pins are BaseMap children (its props are frozen,
// C11). When a station has no known tier (engine unavailable / off-band) it
// renders 'untiered' and stays clickable — distance-only still selects.

import { Marker } from 'react-leaflet';
import { BaseMap } from '../map/BaseMap';
import { gridToLatLon } from '../forms/position/maidenhead';
import { tierColorVar, type ReachTier } from './reachability';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

export interface StationFinderMapProps {
  stations: Station[];
  operatorGrid: string;
  tiers: Map<string, ReachTier>;
  selectedKey: string | null;
  onSelect: (station: Station) => void;
}

export function StationFinderMap(props: StationFinderMapProps) {
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  return (
    <div className="station-finder__map" data-testid="station-map">
      <BaseMap initialCenter={me ?? undefined} initialZoom={2}>
        {me && (
          <Marker position={[me.lat, me.lon]}>
            <span data-testid="me-pin" className="station-finder__me" />
          </Marker>
        )}
        {props.stations.map((s) => {
          const ll = gridToLatLon(s.grid);
          if (!ll) return null;
          const key = stationKey(s);
          const tier = props.tiers.get(key);
          const cls = tier ? `station-finder__pin station-finder__pin--${tier}` : 'station-finder__pin station-finder__pin--untiered';
          return (
            <Marker key={key} position={[ll.lat, ll.lon]}>
              <button
                type="button"
                data-testid="station-pin"
                className={`${cls}${props.selectedKey === key ? ' is-selected' : ''}`}
                style={tier ? { ['--pin-color' as string]: tierColorVar(tier) } : undefined}
                onClick={() => props.onSelect(s)}
                title={`${s.baseCallsign} · ${s.grid}`}
              >
                <span className="station-finder__pin-dot" />
                <span className="station-finder__pin-tag">{s.baseCallsign}</span>
              </button>
            </Marker>
          );
        })}
      </BaseMap>
      <div className="station-finder__reachkey" aria-hidden>
        <span className="k good" /> good
        <span className="k fair" /> fair
        <span className="k marginal" /> marginal
        <span className="k skip" /> unlikely
      </div>
    </div>
  );
}
```

> **If the leaflet mock does not render `<Marker>` children as DOM** (so `station-pin` testids don't appear), fall back to rendering the pins in an absolutely-positioned overlay `<div>` sibling to `<BaseMap>` using `projection.ts`'s lat/lon→pixel helper. Read `src/map/projection.ts` and `src/map/testMapMock.ts` Step-3 output before choosing. Either way the testids + classes above are the contract the tests assert.

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/StationFinderMap.test.tsx`
Expected: PASS. (If the mock's `<Marker>` doesn't expose children, apply the overlay fallback above and re-run.)

- [ ] **Step 5: Commit**

```bash
git add src/catalog/StationFinderMap.tsx src/catalog/StationFinderMap.test.tsx
git commit -m "feat(catalog): reachability-weighted station map pane (tuxlink-gife)"
```

---

### Task 10: Right rail (header + aiming + propagation + channels)

**Files:**
- Create: `src/catalog/StationRail.tsx`
- Test: `src/catalog/StationRail.test.tsx`
- Reuses: `groupChannelsByMode`/`channelToDial`/`channelReliability` (Task 7); `bestBandNow` (Task 2); `bandForKhz`/`bandLabel` (Task 1); `emitGatewayPrefill` (`../favorites/prefillEvent`); `PathPrediction` (Task 3); `Station` (Task 4).

**Context (spec §7 right rail):** (1) selected-station header (call, sysop, location/grid, mode badges); (2) antenna-aiming hero (compass + bearing° + distance — from the prediction when available, else from `distanceFromGrids`/great-circle bearing); (3) path propagation forecast (per-band bars + best-band-now + 24h sparkline) — only when prediction `ok`; (4) channels grouped by mode/frequency with per-channel reliability pip + `Use →`. `Use →` emits `emitGatewayPrefill(channelToDial(...))`; it is enabled only when the channel's mode equals `activePrefillMode` (the open modem), else disabled with a hint. Empty state when no station selected.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/StationRail.test.tsx
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { GATEWAY_PREFILL_EVENT } from '../favorites/prefillEvent';
import { StationRail } from './StationRail';
import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';

const station: Station = {
  baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug Jarmuth', location: 'Wickenburg, AZ',
  modes: ['vara-hf', 'ardop-hf', 'packet'], fetchedAtMs: 1,
  channels: [
    { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
    { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'ardop-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
  ],
};
const prediction: PathPrediction = {
  bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
  channels: [
    { frequencyKhz: 3590, voacapMhz: 4, relByHour: Array(24).fill(0.74), snrByHour: Array(24).fill(10), mufdayByHour: Array(24).fill(0.9) },
    { frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(15), mufdayByHour: Array(24).fill(1) },
  ],
};

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.restoreAllMocks());

describe('StationRail', () => {
  it('shows an empty state when no station is selected', () => {
    render(<StationRail station={null} prediction={null} predictionStatus="idle" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/select a station/i)).toBeTruthy();
  });

  it('renders the selected-station header', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText('N0DAJ')).toBeTruthy();
    expect(screen.getByText(/Doug Jarmuth/)).toBeTruthy();
    expect(screen.getByText(/Wickenburg, AZ/)).toBeTruthy();
  });

  it('shows bearing + distance from the prediction', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/318°/)).toBeTruthy();
    expect(screen.getByTestId('aim-distance').textContent).toMatch(/\d+ mi/);
  });

  it('renders the path forecast with best-band-now when prediction is ok', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/best now: 40 m/i)).toBeTruthy();
  });

  it('hides the forecast and shows a degrade note when prediction is unavailable', () => {
    render(<StationRail station={station} prediction={null} predictionStatus="unavailable" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.queryByText(/best now/i)).toBeNull();
    expect(screen.getByText(/forecast unavailable/i)).toBeTruthy();
  });

  it('groups channels by mode and shows per-channel reliability', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText('VARA HF')).toBeTruthy();
    expect(screen.getByText('ARDOP HF')).toBeTruthy();
    // 7.103 VARA shows 86%
    expect(screen.getAllByText(/86%/).length).toBeGreaterThan(0);
  });

  it('Use → emits a prefill dial for a channel matching the active modem', () => {
    const handler = vi.fn();
    window.addEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    const vara40 = screen.getByTestId('use-vara-hf-7103');
    fireEvent.click(vara40);
    expect(handler).toHaveBeenCalled();
    const evt = handler.mock.calls[0][0] as CustomEvent;
    expect(evt.detail).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
    window.removeEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
  });

  it('disables Use → for channels whose mode is not the active modem', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByTestId('use-ardop-hf-7103').hasAttribute('disabled')).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/StationRail.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/catalog/StationRail.tsx
// Right rail for Find-a-Station (design §7): selected-station header → antenna
// aiming hero → path propagation forecast → channels grouped by mode/frequency
// with per-channel reliability + Use →. Replaces the old redundant station list.
// Use → emits emitGatewayPrefill for a channel matching the open modem; other
// channels are listed but their Use → is disabled with a hint (RADIO-1: this
// only fills a form — the operator still clicks Connect).

import { groupChannelsByMode, channelToDial, channelReliability } from './channelGrouping';
import { bestBandNow } from './reachability';
import { bandForKhz, bandLabel, HF_BANDS } from './bandPlan';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import { distanceFromGrids, kmToMi } from './distance';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { RadioMode } from '../favorites/types';

export interface StationRailProps {
  station: Station | null;
  prediction: PathPrediction | null;
  predictionStatus: PredictionStatus;
  operatorGrid: string;
  utcHour: number;
  /** The open modem that can consume a prefill, or undefined if none. */
  activePrefillMode?: RadioMode;
}

const mhz = (khz: number): string => (khz / 1000).toFixed(3);
const MODE_LABEL: Record<string, string> = { 'vara-hf': 'VARA HF', 'ardop-hf': 'ARDOP HF', packet: 'Packet', pactor: 'Pactor', 'robust-packet': 'Robust Packet' };

/** Great-circle bearing from two grids (deg), for the distance-only fallback. */
function bearingFromGrids(a: string, b: string): number | null {
  const pa = gridToLatLon(a), pb = gridToLatLon(b);
  if (!pa || !pb) return null;
  const φ1 = (pa.lat * Math.PI) / 180, φ2 = (pb.lat * Math.PI) / 180;
  const Δλ = ((pb.lon - pa.lon) * Math.PI) / 180;
  const y = Math.sin(Δλ) * Math.cos(φ2);
  const x = Math.cos(φ1) * Math.sin(φ2) - Math.sin(φ1) * Math.cos(φ2) * Math.cos(Δλ);
  return (((Math.atan2(y, x) * 180) / Math.PI) + 360) % 360;
}

export function StationRail(props: StationRailProps) {
  const { station, prediction, predictionStatus, operatorGrid, utcHour, activePrefillMode } = props;

  if (!station) {
    return <div className="station-finder__rail station-finder__rail--empty">Select a station on the map.</div>;
  }

  const bearing = prediction?.bearingDeg ?? (operatorGrid ? bearingFromGrids(operatorGrid, station.grid) : null);
  const distKm = prediction?.distanceKm ?? (operatorGrid ? distanceFromGrids(operatorGrid, station.grid) : null);
  const distMi = distKm != null ? Math.round(kmToMi(distKm)) : null;
  const best = prediction ? bestBandNow(prediction, utcHour) : null;

  const onUse = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (dial) emitGatewayPrefill(dial);
  };

  return (
    <div className="station-finder__rail">
      <header className="station-finder__sta">
        <div className="station-finder__sta-top">
          <span className="station-finder__call">{station.baseCallsign}</span>
          <span className="station-finder__badges">
            {station.modes.map((m) => (
              <span key={m} className={`station-finder__mb station-finder__mb--${m}`}>{MODE_LABEL[m] ?? m}</span>
            ))}
          </span>
        </div>
        <div className="station-finder__who">
          {[station.sysopName, station.location, station.grid].filter(Boolean).join(' · ')}
        </div>
      </header>

      <div className="station-finder__aim">
        <div className="station-finder__compass" style={bearing != null ? { ['--bearing' as string]: `${bearing}deg` } : undefined} aria-hidden>
          <span className="station-finder__needle" />
        </div>
        <div>
          <div className="station-finder__big">{bearing != null ? `${Math.round(bearing)}°` : '—'}</div>
          <div className="station-finder__lab">aim antenna</div>
        </div>
        <div className="station-finder__dist" data-testid="aim-distance">
          <div className="station-finder__big">{distMi != null ? `${distMi} mi` : '—'}</div>
          <div className="station-finder__lab">short path</div>
        </div>
      </div>

      {predictionStatus === 'ok' && prediction ? (
        <div className="station-finder__prop">
          <h4>Path forecast · you → {station.baseCallsign}
            {best && <span className="station-finder__best">best now: {bandLabel(best.band)}</span>}
          </h4>
          {HF_BANDS.map((b) => {
            const pc = prediction.channels.find((c) => bandForKhz(c.frequencyKhz) === b);
            const rel = pc ? pc.relByHour[utcHour] ?? 0 : null;
            return (
              <div key={b} className={`station-finder__pbar${best?.band === b ? ' is-current' : ''}`}>
                <span className="station-finder__bn">{bandLabel(b)}</span>
                <div className="station-finder__track">
                  <div className="station-finder__fill" style={{ width: `${Math.round((rel ?? 0) * 100)}%` }} />
                </div>
                <span className="station-finder__pct">{rel != null ? `${Math.round(rel * 100)}%` : '—'}</span>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="station-finder__prop station-finder__prop--degraded">
          {predictionStatus === 'no-location'
            ? 'Set your location in the status bar to see the path forecast.'
            : 'Forecast unavailable — showing channels without reliability.'}
        </div>
      )}

      <div className="station-finder__channels">
        {groupChannelsByMode(station).map((group) => (
          <div key={group.mode}>
            <div className="station-finder__chh">
              <span className={`station-finder__sw station-finder__sw--${group.mode}`} />
              {MODE_LABEL[group.mode] ?? group.mode}
              <span className="station-finder__chh-n">{group.channels.length} ch</span>
            </div>
            {group.channels.map((ch) => {
              const rel = prediction ? channelReliability(ch, prediction, utcHour) : null;
              const dialable = channelToDial(station, ch) != null;
              const active = activePrefillMode != null && ch.mode === activePrefillMode;
              return (
                <div key={`${ch.mode}-${ch.frequencyKhz}-${ch.ssid ?? ''}`} className={`station-finder__ch${rel?.tier === 'skip' ? ' is-dim' : ''}`}>
                  <span className="station-finder__rel" style={rel ? { background: `var(--reach-${rel.tier})` } : undefined} />
                  <div>
                    <div className="station-finder__f">{mhz(ch.frequencyKhz)} MHz</div>
                    <div className="station-finder__sub">
                      {ch.band === 'vhf-uhf' ? `VHF/UHF · local${ch.ssid ? ` · connect ${ch.ssid}` : ''}` : bandLabel(ch.band ?? '40m')}
                    </div>
                  </div>
                  <span className="station-finder__q">{rel ? `${Math.round(rel.rel * 100)}%` : ch.band === 'vhf-uhf' ? 'LoS?' : '—'}</span>
                  <button
                    type="button"
                    data-testid={`use-${ch.mode}-${ch.frequencyKhz}`}
                    className="station-finder__use"
                    disabled={!dialable || !active}
                    title={!dialable ? 'No modem for this mode' : !active ? `Open the ${MODE_LABEL[ch.mode]} modem to use this channel` : undefined}
                    onClick={() => onUse(ch)}
                  >
                    Use →
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/catalog/StationRail.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/StationRail.tsx src/catalog/StationRail.test.tsx
git commit -m "feat(catalog): Find-a-Station right rail (aiming + forecast + channels) (tuxlink-gife)"
```

---

### Task 11: Panel assembly + CSS (the StationFinderPanel)

**Files:**
- Create: `src/catalog/StationFinderPanel.tsx`, `src/catalog/StationFinderPanel.css`
- Test: `src/catalog/StationFinderPanel.test.tsx`
- Reuses: `useStations` (`./useStations`), `aggregateStations` (Task 4), `useReachabilityMap` (Task 6), `useStationPrediction` (Task 5), `StationFinderControls` (Task 8), `StationFinderMap` (Task 9), `StationRail` (Task 10). Mirrors the old `CatalogBuilderPanel`'s overlay/escape/close behaviour + the z-index-above-chrome invariant (`.station-finder-overlay`).

**Context:** The panel owns: operator grid (from `config_read`, full precision), selected band (default 40 m), enabled modes (default all three), selected station, and the UTC hour (computed once on open). On open it fetches the three modes' listings (offline-first: U2 seeds the cache so this shows last-known-good immediately), aggregates to stations, filters by enabled modes, ranks via `useReachabilityMap`, and predicts the selected station via `useStationPrediction`. `activePrefillMode` passes through to the rail.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/StationFinderPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
import { resetMapMock } from '../map/testMapMock';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { StationFinderPanel } from './StationFinderPanel';

function renderPanel(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const N0DAJ = { channel: 'N0DAJ', callsign: 'N0DAJ', sysopName: 'Doug', grid: 'DM34oa', location: 'Wickenburg, AZ', frequenciesKhz: [3590, 7103], lastUpdate: null, email: null, homepage: null };

beforeEach(() => {
  resetMapMock();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' };
    if (cmd === 'catalog_fetch_stations') return [{ mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: Date.now(), gateways: [N0DAJ] }];
    if (cmd === 'propagation_predict_path') return { bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6, channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }] };
    return undefined;
  });
});

describe('StationFinderPanel', () => {
  it('renders the Find a Station dialog with the controls bar', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    expect(await screen.findByRole('dialog', { name: /find a station/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /40 m/ })).toBeTruthy();
  });

  it('fetches + aggregates stations and shows a pin', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getAllByTestId('station-pin').length).toBeGreaterThan(0));
  });

  it('selecting a pin populates the right rail', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} activePrefillMode="vara-hf" />);
    const pin = await screen.findByTestId('station-pin');
    fireEvent.click(pin);
    expect(await screen.findByText('N0DAJ')).toBeTruthy();
    expect(await screen.findByText(/Doug/)).toBeTruthy();
  });

  it('closes on the × button', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    fireEvent.click(await screen.findByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalled();
  });

  it('closes on Escape', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    await screen.findByRole('dialog');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/catalog/StationFinderPanel.test.tsx`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement the panel**

```typescript
// src/catalog/StationFinderPanel.tsx
// Find a Station — propagation-aware station finder (design §7, Mock-D).
// Supersedes CatalogBuilderPanel. Inline overlay (no pop-up window). Owns the
// operator grid (from config_read), band (default 40 m), mode filter (default
// all three prefillable modes), and the selected station. Offline-first: U2
// seeds the station cache so the list shows immediately; reachability + the
// per-path forecast light up when U1 prediction is available and degrade to
// distance-only otherwise. RADIO-1: nothing here transmits.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useStations } from './useStations';
import { aggregateStations, type Station } from './stationModel';
import { useReachabilityMap, stationKey } from './useReachabilityMap';
import { useStationPrediction } from './useStationPrediction';
import { StationFinderControls, type FilterMode } from './StationFinderControls';
import { StationFinderMap } from './StationFinderMap';
import { StationRail } from './StationRail';
import type { Band } from './bandPlan';
import type { ListingMode } from './stationTypes';
import type { RadioMode } from '../favorites/types';
import './StationFinderPanel.css';

export interface StationFinderPanelProps {
  onClose: () => void;
  /** The open modem that can consume a channel prefill (Use →). */
  activePrefillMode?: RadioMode;
}

const FILTER_MODES: FilterMode[] = ['vara-hf', 'ardop-hf', 'packet'];
// UTC hour is captured once on open (not a live clock) to keep ranking stable.
function currentUtcHour(): number {
  return new Date().getUTCHours();
}
function localTimeLabel(): string {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function StationFinderPanel({ onClose, activePrefillMode }: StationFinderPanelProps) {
  const [grid, setGrid] = useState('');
  const [band, setBand] = useState<Band>('40m');
  const [enabledModes, setEnabledModes] = useState<Set<FilterMode>>(new Set(FILTER_MODES));
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [utcHour] = useState(currentUtcHour);
  const stations = useStations();

  useEffect(() => {
    invoke<{ grid: string | null }>('config_read').then((c) => { if (c?.grid) setGrid(c.grid); }).catch(() => {});
  }, []);

  // Offline-first: fetch the three prefillable modes on open (U2 seeds cache).
  useEffect(() => { stations.fetch(FILTER_MODES as ListingMode[]); /* eslint-disable-next-line */ }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const allStations = useMemo(() => aggregateStations(stations.listings), [stations.listings]);
  const visible = useMemo(
    () => allStations.filter((s) => s.modes.some((m) => (FILTER_MODES as ListingMode[]).includes(m) && enabledModes.has(m as FilterMode))),
    [allStations, enabledModes],
  );

  const reach = useReachabilityMap(grid, visible, band, utcHour);
  const selected: Station | null = useMemo(
    () => visible.find((s) => stationKey(s) === selectedKey) ?? null,
    [visible, selectedKey],
  );
  const pred = useStationPrediction(grid, selected);

  const ssnAgeDays = pred.prediction ? ssnAge(pred.prediction.year, pred.prediction.month) : null;

  const toggleMode = (m: FilterMode) =>
    setEnabledModes((prev) => { const next = new Set(prev); next.has(m) ? next.delete(m) : next.add(m); return next; });

  return (
    <div className="station-finder-overlay" data-testid="station-finder-overlay" role="dialog" aria-label="Find a Station" onClick={onClose}>
      <div className="station-finder" onClick={(e) => e.stopPropagation()}>
        <header className="station-finder__header">
          <h2>Find a Station</h2>
          <button className="station-finder__close" onClick={onClose} aria-label="Close">×</button>
        </header>

        <StationFinderControls
          band={band}
          onBandChange={setBand}
          enabledModes={enabledModes}
          onToggleMode={toggleMode}
          utcHour={utcHour}
          localTime={localTimeLabel()}
          ssn={pred.prediction?.ssn ?? null}
          ssnAgeDays={ssnAgeDays}
          predictionAvailable={reach.available || pred.status === 'ok'}
          onRefresh={() => stations.fetch(FILTER_MODES as ListingMode[])}
          refreshing={stations.loading}
        />

        <div className="station-finder__body">
          <StationFinderMap
            stations={visible}
            operatorGrid={grid}
            tiers={reach.tiers}
            selectedKey={selectedKey}
            onSelect={(s) => setSelectedKey(stationKey(s))}
          />
          <StationRail
            station={selected}
            prediction={pred.prediction}
            predictionStatus={pred.status}
            operatorGrid={grid}
            utcHour={utcHour}
            activePrefillMode={activePrefillMode}
          />
        </div>
      </div>
    </div>
  );
}

/** Whole months between (year, month) and now, as a rough "solar data N old". */
function ssnAge(year: number, month: number): number {
  const now = new Date();
  const months = (now.getUTCFullYear() - year) * 12 + (now.getUTCMonth() + 1 - month);
  return Math.max(0, months) * 30;
}
```

- [ ] **Step 4: Write the CSS (Mock-D styling + z-index invariant)**

Read the chrome max z-index first so the overlay paints above it (the tuxlink-tsl5 invariant the old panel satisfied):

Run: `rg -n 'z-index' src/shell/chrome/chrome.css | sort -t: -k3 -n | tail -3`

Then create `src/catalog/StationFinderPanel.css` modelled on Mock-D's palette + the old `CatalogBuilderPanel.css` overlay rules. Set `.station-finder-overlay { z-index: <chrome-max + 1>; }`. Define the reachability custom properties used by the pins/pips/bars:

```css
/* src/catalog/StationFinderPanel.css — Find a Station (Mock-D surface). */
:root {
  --reach-good: #46d07f;
  --reach-fair: #c9b23a;
  --reach-marginal: #d2842f;
  --reach-skip: #6c5a5a;
}
.station-finder-overlay {
  position: fixed; inset: 0; display: flex; align-items: center; justify-content: center;
  background: rgba(3, 6, 10, 0.62); z-index: 1000; /* MUST exceed chrome max — verify in Step 4 */
}
.station-finder {
  width: min(1180px, 96vw); max-height: 92vh; display: flex; flex-direction: column;
  background: var(--surface, #0c1620); border: 1px solid var(--border2, #34556f);
  border-radius: 10px; overflow: hidden; box-shadow: 0 18px 60px rgba(0, 0, 0, 0.6);
}
.station-finder__header { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border, #243a52); }
.station-finder__header h2 { margin: 0; font-size: 15px; }
.station-finder__close { margin-left: auto; background: none; border: 1px solid var(--border, #243a52); color: inherit; border-radius: 6px; padding: 2px 9px; cursor: pointer; }
.station-finder__body { display: flex; min-height: 540px; flex: 1; overflow: hidden; }
.station-finder__map { position: relative; flex: 1 1 55%; min-width: 0; border-right: 1px solid var(--border, #243a52); }
.station-finder__rail { flex: 1 1 45%; min-width: 360px; display: flex; flex-direction: column; overflow: auto; }
/* pins */
.station-finder__pin { position: relative; background: none; border: none; cursor: pointer; padding: 0; }
.station-finder__pin-dot { display: block; border-radius: 50%; border: 2px solid #0a121b; background: var(--pin-color, #9fb6cc); width: 14px; height: 14px; }
.station-finder__pin--good .station-finder__pin-dot { width: 18px; height: 18px; }
.station-finder__pin--skip .station-finder__pin-dot { background: transparent; border-style: dashed; opacity: 0.7; }
.station-finder__pin--untiered .station-finder__pin-dot { background: #9fb6cc; }
.station-finder__pin-tag { position: absolute; left: 16px; top: -4px; white-space: nowrap; font-size: 10px; background: rgba(8,16,24,.86); border: 1px solid var(--border, #243a52); border-radius: 4px; padding: 1px 5px; opacity: 0; }
.station-finder__pin:hover .station-finder__pin-tag, .station-finder__pin.is-selected .station-finder__pin-tag { opacity: 1; }
/* rail bits */
.station-finder__aim { display: flex; align-items: center; gap: 14px; padding: 11px 13px; border-bottom: 1px solid var(--border, #243a52); }
.station-finder__big { font-size: 22px; font-weight: 700; }
.station-finder__pbar { display: grid; grid-template-columns: 42px 1fr 38px; gap: 9px; align-items: center; margin: 5px 0; }
.station-finder__track { height: 9px; border-radius: 5px; background: #0b1722; overflow: hidden; }
.station-finder__fill { height: 100%; background: var(--accent, #2f86f0); }
.station-finder__ch { display: grid; grid-template-columns: 10px 1fr auto auto; gap: 9px; align-items: center; padding: 7px 13px; border-bottom: 1px solid #16293a; }
.station-finder__ch.is-dim { opacity: 0.5; }
.station-finder__rel { width: 10px; height: 10px; border-radius: 50%; }
.station-finder__use { font: inherit; font-size: 11.5px; font-weight: 600; color: #fff; background: var(--accent, #2f86f0); border: none; border-radius: 6px; padding: 4px 10px; cursor: pointer; }
.station-finder__use:disabled { opacity: 0.4; cursor: not-allowed; }
/* FZ-M1 compact: stack map over rail on the rugged screen */
@media (max-width: 1365px) and (any-pointer: coarse) {
  .station-finder__body { flex-direction: column; }
  .station-finder__map { flex-basis: auto; height: 46vh; border-right: none; border-bottom: 1px solid var(--border, #243a52); }
  .station-finder__rail { min-width: 0; }
}
```

- [ ] **Step 5: Run tests + typecheck**

Run: `pnpm vitest run src/catalog/StationFinderPanel.test.tsx && pnpm tsc --noEmit`
Expected: PASS + no type errors. (Fix any TS mismatches surfaced; `--noEmit` is the project typecheck.)

- [ ] **Step 6: Commit**

```bash
git add src/catalog/StationFinderPanel.tsx src/catalog/StationFinderPanel.css src/catalog/StationFinderPanel.test.tsx
git commit -m "feat(catalog): assemble StationFinderPanel (Mock-D surface) with FZ-M1 compact (tuxlink-gife)"
```

---

### Task 12: Wire into AppShell, rename menu, widen prefill, delete old panel

**Files:**
- Modify: `src/shell/AppShell.tsx`, `src/shell/chrome/menuModel.ts`
- Delete: `src/catalog/CatalogBuilderPanel.tsx`, `src/catalog/CatalogBuilderPanel.css`, `src/catalog/CatalogBuilderPanel.test.tsx`
- Test: existing `src/shell/chrome/menuModel.test.ts`, `src/shell/chrome/dispatchMenuAction.test.ts` (keep green), plus a new App-level mount test in Task 13.

**Context:** Repoint the lazy import + mount to `StationFinderPanel`; widen `catalogPrefillMode` to include VARA (its modem now consumes prefill — verified). Keep the menu id `menu:tools:find_gateway` (changing it churns the menuModel `EXPECTED_IDS` contract test for no functional gain); change only the label. The #550 pin reverts by deleting `CatalogBuilderPanel` (its sole carrier); `GridPickerOverlay` stays for `GridEdit`.

- [ ] **Step 1: Verify GridPickerOverlay is not imported anywhere else under catalog**

Run: `rg -n 'GridPickerOverlay' src --glob '!*.test.*'`
Expected: only `src/shell/GridPickerOverlay.tsx` (def) + `src/shell/GridEdit.tsx` (user). NOT imported by any catalog file after the old panel is deleted. If a second catalog consumer appears, stop and reassess.

- [ ] **Step 2: Update the menu label**

In `src/shell/chrome/menuModel.ts`, change:
```typescript
    { id: 'menu:tools:find_gateway', label: 'Find a Gateway…' },
```
to:
```typescript
    { id: 'menu:tools:find_gateway', label: 'Find a Station…' },
```

- [ ] **Step 3: Repoint the AppShell lazy import + mount + widen prefill**

In `src/shell/AppShell.tsx`:

Replace the lazy import (lines ~67-69):
```typescript
const CatalogBuilderPanel = lazy(() =>
  import('../catalog/CatalogBuilderPanel').then((m) => ({ default: m.CatalogBuilderPanel })),
);
```
with:
```typescript
const StationFinderPanel = lazy(() =>
  import('../catalog/StationFinderPanel').then((m) => ({ default: m.StationFinderPanel })),
);
```

Widen the prefill-mode derivation (lines ~547-553) so VARA stations are usable (VaraRadioPanel consumes prefill — verified):
```typescript
  const catalogPrefillMode: RadioMode | undefined =
    radioPanelMode?.kind === 'packet' ? 'packet'
    : radioPanelMode?.kind === 'ardop-hf' ? 'ardop-hf'
    : radioPanelMode?.kind === 'vara-hf' ? 'vara-hf'
    : radioPanelMode?.kind === 'vara-fm' ? 'vara-fm'
    : undefined;
```
(Ensure `RadioMode` is imported in AppShell: `import type { RadioMode } from '../favorites/types';` — add if absent.)

Replace the mount (lines ~1307-1312):
```typescript
      {catalogBuilderOpen && (
        <Suspense fallback={null}>
          <CatalogBuilderPanel
            activePrefillMode={catalogPrefillMode}
            onClose={() => setCatalogBuilderOpen(false)}
          />
        </Suspense>
      )}
```
with:
```typescript
      {catalogBuilderOpen && (
        <Suspense fallback={null}>
          <StationFinderPanel
            activePrefillMode={catalogPrefillMode}
            onClose={() => setCatalogBuilderOpen(false)}
          />
        </Suspense>
      )}
```
(Leave the `catalogBuilderOpen` state name + `openCatalogBuilder` handler as-is — internal names; renaming them is churn with no user-facing effect. Match whatever the exact surrounding `Suspense`/conditional shape is in the file.)

- [ ] **Step 4: Delete the superseded panel**

```bash
git rm src/catalog/CatalogBuilderPanel.tsx src/catalog/CatalogBuilderPanel.css src/catalog/CatalogBuilderPanel.test.tsx
```

- [ ] **Step 5: Run the affected tests + typecheck + grep for dangling refs**

Run:
```bash
rg -n 'CatalogBuilderPanel' src   # expect: no matches
pnpm vitest run src/shell/chrome/menuModel.test.ts src/shell/chrome/dispatchMenuAction.test.ts
pnpm tsc --noEmit
```
Expected: no `CatalogBuilderPanel` references remain; menu + dispatch tests PASS; no type errors. If `menuModel.test.ts` asserts the label text, update its expectation to `Find a Station…`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(shell): wire StationFinderPanel into AppShell; rename menu to Find a Station; widen prefill to VARA; remove CatalogBuilderPanel + #550 pin (tuxlink-gife)"
```

---

### Task 13: App-level mount test (production path) + full gate

**Files:**
- Create: `src/catalog/StationFinderPanel.appmount.test.tsx` (App-level mount guards the production context providers — per `test-the-production-mount-path` memory: unit tests inject providers a real mount might lack).
- Run: the full CI-equivalent gate.

**Context:** A panel-level test renders inside a hand-rolled `QueryClientProvider`; production mounts inside AppShell's real provider tree + Suspense. This test mounts the panel the way production does (lazy + Suspense + a QueryClient) and asserts no missing-provider crash, catching the class of bug where a unit test silently supplies context production fails to provide.

- [ ] **Step 1: Write the failing test**

```typescript
// src/catalog/StationFinderPanel.appmount.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Suspense, lazy } from 'react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

// Mirror AppShell's lazy import exactly.
const StationFinderPanel = lazy(() => import('./StationFinderPanel').then((m) => ({ default: m.StationFinderPanel })));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' };
    if (cmd === 'catalog_fetch_stations') return [];
    return undefined;
  });
});

describe('StationFinderPanel — production mount path', () => {
  it('mounts under lazy + Suspense + QueryClientProvider without a missing-provider crash', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <Suspense fallback={<div>loading</div>}>
          <StationFinderPanel onClose={() => {}} />
        </Suspense>
      </QueryClientProvider>,
    );
    await waitFor(() => expect(screen.getByRole('dialog', { name: /find a station/i })).toBeTruthy());
  });
});
```

- [ ] **Step 2: Run it**

Run: `pnpm vitest run src/catalog/StationFinderPanel.appmount.test.tsx`
Expected: PASS (the panel only needs a QueryClient + the tauri mock; this proves it).

- [ ] **Step 3: Commit**

```bash
git add src/catalog/StationFinderPanel.appmount.test.tsx
git commit -m "test(catalog): App-level production-mount guard for StationFinderPanel (tuxlink-gife)"
```

- [ ] **Step 4: Run the FULL gate (CI-equivalent — the operator's required pre-push gate)**

Run (frontend):
```bash
pnpm vitest run
pnpm tsc --noEmit
pnpm lint    # if the project defines it (check package.json scripts)
```
Run (backend — unchanged by U3, but the gate is full per memory scoped-vitest-misses-contract-tests):
```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --doc
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Expected: ALL green. Re-run clippy until exit 0 (it hides later-target lints behind the first failure). Fix anything red before proceeding. Reap any vitest worker zombies after the sweep: `pgrep -f vitest` should be empty; `pkill -9 -f vitest` if not.

- [ ] **Step 5: Update the implementation log**

Prepend an entry to `dev/implementation-log.md` (create if absent) dated 2026-06-11 summarizing: U3 Find-a-Station map UI shipped — new prediction TS binding, station/channel aggregation, reachability ranking, Mock-D panel; superseded CatalogBuilderPanel + reverted #550 pin; widened prefill to VARA.

```bash
git add dev/implementation-log.md
git commit -m "docs: log U3 Find-a-Station map UI (tuxlink-gife)"
```

---

## Out of scope / follow-ups (file as bd issues at session end)

- **`tuxlink-hhxs` (U1 packaging):** bundle voacapl + itshfbc into the .deb so prediction works in a real build. OPERATOR DECISION (arm64 CI strategy). Until done, U3 ships and *degrades to distance-only* in a packaged build — functional, but the propagation lights stay off until packaging lands. The feature is not "fully shipped" per the DoD until both U3 merges AND U1 is packaged.
- **Live SFI/K-index + real-time solar feed:** U1 exposes only bundled SSN. SFI/K-index degrade (omitted) in U3. A live solar feed (cached offline like the station list) is a later enhancement.
- **Area-coverage map ranking (§5 option b):** v1 uses point-to-point per station. If station counts or recompute cadence cause jank in a real build, switch to a sampled reliability grid.
- **24h reliability sparkline (Mock-D detail):** the per-path forecast renders per-band bars + best-band-now; the decorative 24h sparkline is deferred unless trivial to add from `relByHour` of the best band (it is — add if time permits in Task 10, else follow-up).

## Self-review notes (author)

- **Spec coverage:** §7 map (Task 9) + right rail aiming/forecast/channels (Task 10) + top bar (Task 8) + assembly/FZ-M1 (Task 11); §8 data model (Tasks 4, 7); §11 decisions 1-9 all map to tasks (1=no location-setter → Task 11 uses config grid, no field; 2=pin/channel model → Tasks 4/7; 3=reachability ranking → Tasks 2/6/9; 4=voacapl/SSN → Tasks 3/5/8; 5=no SPLAT/VHF factual → Tasks 1/7/10; 6=persistent cache open → U2 already, consumed in Task 11; 7=bearing+forecast rail → Task 10; 8=channels by frequency → Task 7; 9=supersede + revert #550 + favorites/prefill → Task 12 + prefill in Task 10). §12 open items all decided at top.
- **Type consistency:** `Station`/`Channel` (Task 4) used identically in 5-7, 9-11; `PathPrediction`/`ChannelReliability` (Task 3) used in 2, 5, 6, 7, 10; `ReachTier` (Task 2) in 6, 9; `stationKey` defined in Task 6, imported by 9 + 11; `FilterMode` defined in Task 8, imported by 11.
- **Favorites/★ carry-over:** the spec says favorites carry over. The Mock-D surface does not foreground a ★ control; favorites + recents still function via the existing FavoritesTabs in the radio dock (unchanged). A ★-on-station affordance in the rail is a reasonable add but not in Mock-D; deferred to avoid scope creep beyond the approved surface. **Flag for operator at handoff.**
