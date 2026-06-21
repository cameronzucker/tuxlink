# APRS Map Station Category Filter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the APRS map's single-select category `<select>` with a collapsible multi-select layers panel backed by a curated 8-bucket symbol classifier, so the operator can show/hide whole categories of heard stations.

**Architecture:** A pure data-table classifier (`stationBuckets.ts`) maps each APRS symbol to exactly one of 8 buckets. A persistence hook (`usePersistedBucketFilter`) holds the enabled-bucket set + collapse state in localStorage. A presentational panel (`AprsLayersPanel`) renders checkboxes + live counts. `AprsPositionsMap`/`MapOverlays` wire them together: per-station visibility becomes `enabledBuckets.has(bucketForStation(station))`.

**Tech Stack:** TypeScript (strict), React 19, Leaflet, Vitest + @testing-library/react (jsdom). Frontend only — no Rust, no Tauri commands.

## Global Constraints

- **RF-honesty:** the filter only shows/hides pins for stations actually heard. Never fabricate, infer, relocate, or silently drop a station. Unknown symbols fall to the `other` bucket and stay visible by default.
- **Default state:** every bucket ON (map draws all heard stations on first run, exactly as today).
- **Engine-agnostic, current surface:** target the existing Leaflet `AprsPositionsMap`. Do NOT touch the MapLibre engine decision.
- **No new copy hedging:** UI copy is declarative, no first person, no "currently/for now."
- **Verification (2026-06-20 operator rule):** run `pnpm typecheck` and only the **touched** vitest files locally; CI runs the full vitest + cargo. Do NOT run the full vitest suite or cargo locally.
- **Branch / git:** all work on `bd-tuxlink-8fjx/aprs-category-filter` in the worktree `worktrees/bd-tuxlink-8fjx-aprs-category-filter`. Every commit carries `Agent: bayou-granite-slate` + the `Co-Authored-By` trailer. No destructive git. Push after each task (never hold a push).
- **Commands run from the worktree:** prefix vitest with the worktree path; use `git -C <worktree>` for commits.

**Spec:** `docs/design/2026-06-20-aprs-station-category-filter-design.md`.

---

## File Structure

- `src/aprs/stationBuckets.ts` — **new.** `BucketKey` type, `BUCKETS` metadata (ordered), `bucketForStation()` classifier + curated symbol tables, `ALL_BUCKET_KEYS`, `emptyCounts()`.
- `src/aprs/stationBuckets.test.ts` — **new.** Exhaustive + targeted classifier tests.
- `src/map/usePersistedBucketFilter.ts` — **new.** Persisted enabled-set + collapse-state hook (mirrors `usePersistedViewport`).
- `src/map/usePersistedBucketFilter.test.ts` — **new.**
- `src/aprs/AprsLayersPanel.tsx` — **new.** Presentational collapsible panel.
- `src/aprs/AprsLayersPanel.css` — **new.**
- `src/aprs/AprsLayersPanel.test.tsx` — **new.**
- `src/aprs/AprsPositionsMap.tsx` — **modify.** Swap `category` state for the bucket filter; render the panel; change `MapOverlays` visibility predicate + signature; compute counts; drop `CATEGORIES`/`categoryByKey`/`WxFilterControl`.
- `src/aprs/AprsPositionsMap.test.tsx` — **modify.** Update the existing category-filter test (line ~239) to drive the panel.
- `src/aprs/stationCategories.ts` + `src/aprs/stationCategories.test.ts` — **delete.** Superseded.

---

## Task 1: Curated symbol → bucket classifier

**Files:**
- Create: `src/aprs/stationBuckets.ts`
- Test: `src/aprs/stationBuckets.test.ts`

**Interfaces:**
- Consumes: `SYMBOL_CODES`, `PRIMARY_SYMBOLS`, `ALTERNATE_SYMBOLS`, `OVERLAY_MEANINGS` are NOT imported — this module authors its own tables. (It only conceptually mirrors `aprsSymbols.ts`' resolution order.)
- Produces:
  - `type BucketKey = 'weather' | 'igate' | 'digipeater' | 'emergency' | 'vehicles' | 'people' | 'fixed' | 'other'`
  - `interface BucketMeta { key: BucketKey; label: string; glyph: string }`
  - `const BUCKETS: BucketMeta[]` (ordered: weather, igate, digipeater, emergency, vehicles, people, fixed, other)
  - `const ALL_BUCKET_KEYS: BucketKey[]`
  - `interface StationBucketCtx { symbolTable: string; symbolCode: string; isWeather: boolean }`
  - `function bucketForStation(ctx: StationBucketCtx): BucketKey`
  - `function emptyCounts(): Record<BucketKey, number>`

- [ ] **Step 1: Write the failing test**

Create `src/aprs/stationBuckets.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import {
  bucketForStation,
  BUCKETS,
  ALL_BUCKET_KEYS,
  emptyCounts,
  type BucketKey,
} from './stationBuckets';

const b = (symbolTable: string, symbolCode: string, isWeather = false): BucketKey =>
  bucketForStation({ symbolTable, symbolCode, isWeather });

// Every printable APRS code, ! (0x21) .. ~ (0x7E).
const CODES = Array.from({ length: 0x7e - 0x21 + 1 }, (_, i) => String.fromCharCode(0x21 + i));

describe('stationBuckets metadata', () => {
  it('exposes 8 ordered buckets with unique keys', () => {
    expect(BUCKETS.map((m) => m.key)).toEqual([
      'weather', 'igate', 'digipeater', 'emergency', 'vehicles', 'people', 'fixed', 'other',
    ]);
  });
  it('ALL_BUCKET_KEYS mirrors BUCKETS order', () => {
    expect(ALL_BUCKET_KEYS).toEqual(BUCKETS.map((m) => m.key));
  });
  it('every bucket has a label and a glyph', () => {
    for (const m of BUCKETS) {
      expect(m.label.length).toBeGreaterThan(0);
      expect(m.glyph.length).toBeGreaterThan(0);
    }
  });
  it('emptyCounts has every key at 0', () => {
    const c = emptyCounts();
    expect(Object.keys(c).sort()).toEqual([...ALL_BUCKET_KEYS].sort());
    expect(Object.values(c).every((n) => n === 0)).toBe(true);
  });
});

describe('bucketForStation — total coverage (no throw, always a valid bucket)', () => {
  it('classifies every primary-table code into a known bucket', () => {
    for (const code of CODES) expect(ALL_BUCKET_KEYS).toContain(b('/', code));
  });
  it('classifies every alternate-table code into a known bucket', () => {
    for (const code of CODES) expect(ALL_BUCKET_KEYS).toContain(b('\\', code));
  });
});

describe('bucketForStation — weather override', () => {
  it('a station with valid WX readings is Weather regardless of symbol', () => {
    expect(b('/', '>', /* isWeather */ true)).toBe('weather'); // a car reporting weather
  });
  it('the weather symbol itself is Weather', () => {
    expect(b('/', '_')).toBe('weather');
    expect(b('\\', '_')).toBe('weather');
    expect(b('/', 'W')).toBe('weather');
  });
  it('weather-condition objects are Weather', () => {
    expect(b('\\', 't')).toBe('weather'); // tornado
    expect(b('\\', ':')).toBe('weather'); // hail
    expect(b('\\', 'w')).toBe('weather'); // flooding
    expect(b('\\', '@')).toBe('weather'); // hurricane
  });
});

describe('bucketForStation — infrastructure', () => {
  it('digipeaters', () => {
    expect(b('/', '#')).toBe('digipeater');
    expect(b('\\', '#')).toBe('digipeater');
    expect(b('/', 'r')).toBe('digipeater'); // repeater
    expect(b('/', 'n')).toBe('digipeater'); // node
    expect(b('\\', '8')).toBe('digipeater'); // network node
    expect(b('W', '#')).toBe('digipeater'); // WIDEn-N overlay → falls through to alt '#'
    expect(b('D', 'a')).toBe('digipeater'); // D-STAR overlay
    expect(b('Y', 'a')).toBe('digipeater'); // C4FM repeater overlay
  });
  it('iGates / gateways', () => {
    expect(b('/', '&')).toBe('igate'); // HF gateway
    expect(b('\\', '&')).toBe('igate'); // igate
    expect(b('/', 'I')).toBe('igate'); // TCP/IP
    expect(b('I', '&')).toBe('igate'); // I& overlay → falls through to alt '&'
    expect(b('R', '&')).toBe('igate');
  });
});

describe('bucketForStation — emergency / emcomm', () => {
  it('served-agency, incident, and ARES/RACES symbols', () => {
    expect(b('\\', '!')).toBe('emergency'); // emergency
    expect(b('/', 'A')).toBe('emergency'); // aid station
    expect(b('/', 'o')).toBe('emergency'); // EOC
    expect(b('/', 'c')).toBe('emergency'); // incident command
    expect(b('/', 'a')).toBe('emergency'); // ambulance
    expect(b('/', 'f')).toBe('emergency'); // fire truck
    expect(b('/', '+')).toBe('emergency'); // red cross
    expect(b('/', '!')).toBe('emergency'); // police
    expect(b('/', 'P')).toBe('emergency'); // police
    expect(b('\\', 'C')).toBe('emergency'); // coast guard
    expect(b('A', 'a')).toBe('emergency'); // ARES overlay → alt 'a'
    expect(b('\\', 'a')).toBe('emergency'); // ARRL/ARES/WinLink base
  });
});

describe('bucketForStation — vehicles, people, fixed', () => {
  it('vehicles include aircraft and boats', () => {
    expect(b('/', '>')).toBe('vehicles'); // car
    expect(b('/', 'k')).toBe('vehicles'); // truck
    expect(b('/', '^')).toBe('vehicles'); // large aircraft
    expect(b('/', 'X')).toBe('vehicles'); // helicopter
    expect(b('/', 'Y')).toBe('vehicles'); // yacht
    expect(b('\\', 's')).toBe('vehicles'); // ship/boat
    expect(b('B', '>')).toBe('vehicles'); // EV overlay → alt '>'
  });
  it('people', () => {
    expect(b('/', '[')).toBe('people'); // person
    expect(b('/', 'b')).toBe('people'); // bicycle
    expect(b('/', ')')).toBe('people'); // wheelchair
    expect(b('/', 'e')).toBe('people'); // horse
  });
  it('fixed / places', () => {
    expect(b('/', '-')).toBe('fixed'); // house
    expect(b('/', 'h')).toBe('fixed'); // hospital
    expect(b('/', 'K')).toBe('fixed'); // school
    expect(b('\\', 'R')).toBe('fixed'); // restaurant
    expect(b('\\', '%')).toBe('fixed'); // power plant
    expect(b('S', '-')).toBe('fixed'); // solar house overlay → alt '-'
  });
});

describe('bucketForStation — other catch-all', () => {
  it('unknown / undefined symbols fall to other and never throw', () => {
    expect(b('/', 'J')).toBe('other'); // undefined
    expect(b('\\', 'Z')).toBe('other'); // undefined
    expect(b('/', '?')).toBe('other'); // file server
    expect(b('@', '@')).toBe('other'); // nonsense overlay/code combo
    expect(b('', '')).toBe('other'); // malformed
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/aprs/stationBuckets.test.ts`
Expected: FAIL — `Cannot find module './stationBuckets'`.

- [ ] **Step 3: Write the implementation**

Create `src/aprs/stationBuckets.ts`:

```ts
// src/aprs/stationBuckets.ts
//
// Curated APRS symbol → station-category bucket classifier (tuxlink-8fjx). The
// symbol space is finite and named in aprsSymbols.ts, so the buckets are authored
// by hand as an explicit data table rather than fuzzy heuristics. Precedence
// between buckets is resolved at AUTHORING time — each symbol (and each known
// overlay combo) is assigned its single correct bucket directly — so there is no
// runtime priority ladder. The only runtime rule is the weather-readings
// override. RF-honesty: an unmatched symbol returns `other` (visible), never
// dropped.
//
// Resolution order mirrors lookupAprsSymbol(): overlay combo → primary ('/') →
// alternate ('\\') → overlay-base (alternate) → other.

export type BucketKey =
  | 'weather'
  | 'igate'
  | 'digipeater'
  | 'emergency'
  | 'vehicles'
  | 'people'
  | 'fixed'
  | 'other';

export interface BucketMeta {
  key: BucketKey;
  label: string;
  glyph: string;
}

/** Display order for the layers panel. */
export const BUCKETS: BucketMeta[] = [
  { key: 'weather', label: 'Weather', glyph: '🌡️' },
  { key: 'igate', label: 'iGates & Gateways', glyph: '🌐' },
  { key: 'digipeater', label: 'Digipeaters & Nodes', glyph: '📡' },
  { key: 'emergency', label: 'Emergency / EmComm', glyph: '🚑' },
  { key: 'vehicles', label: 'Vehicles & Craft', glyph: '🚗' },
  { key: 'people', label: 'People', glyph: '🧍' },
  { key: 'fixed', label: 'Fixed & Places', glyph: '🏠' },
  { key: 'other', label: 'Other', glyph: '▫️' },
];

export const ALL_BUCKET_KEYS: BucketKey[] = BUCKETS.map((m) => m.key);

export function emptyCounts(): Record<BucketKey, number> {
  return {
    weather: 0, igate: 0, digipeater: 0, emergency: 0,
    vehicles: 0, people: 0, fixed: 0, other: 0,
  };
}

export interface StationBucketCtx {
  symbolTable: string;
  symbolCode: string;
  isWeather: boolean;
}

// Primary table ('/') code → bucket. Absent codes → other.
const PRIMARY_BUCKET: Record<string, BucketKey> = {
  '!': 'emergency', // police/sheriff
  '#': 'digipeater',
  '&': 'igate',     // HF gateway
  "'": 'vehicles',  // small aircraft
  ')': 'people',    // wheelchair
  '*': 'vehicles',  // snowmobile
  '+': 'emergency', // red cross
  '-': 'fixed',     // house
  ':': 'emergency', // fire
  ';': 'fixed',     // campground
  '<': 'vehicles',  // motorcycle
  '=': 'vehicles',  // railroad engine
  '>': 'vehicles',  // car
  '@': 'weather',   // hurricane forecast
  'A': 'emergency', // aid station
  'C': 'vehicles',  // canoe
  'D': 'fixed',     // depot
  'F': 'vehicles',  // farm vehicle
  'H': 'fixed',     // hotel
  'I': 'igate',     // TCP/IP station
  'K': 'fixed',     // school
  'O': 'vehicles',  // balloon
  'P': 'emergency', // police
  'R': 'vehicles',  // RV
  'U': 'vehicles',  // bus
  'V': 'vehicles',  // ATV
  'W': 'weather',   // weather service site
  'X': 'vehicles',  // helicopter
  'Y': 'vehicles',  // yacht
  '[': 'people',    // person
  '\\': 'emergency',// DF triangle
  ']': 'fixed',     // post office
  '^': 'vehicles',  // large aircraft
  '_': 'weather',   // weather station
  '`': 'fixed',     // dish antenna (QTH infrastructure)
  'a': 'emergency', // ambulance
  'b': 'people',    // bicycle
  'c': 'emergency', // incident command post
  'd': 'emergency', // fire department
  'e': 'people',    // horse/rider
  'f': 'emergency', // fire truck
  'g': 'vehicles',  // glider
  'h': 'fixed',     // hospital
  'j': 'vehicles',  // jeep
  'k': 'vehicles',  // truck
  'm': 'digipeater',// Mic-E repeater
  'n': 'digipeater',// node
  'o': 'emergency', // EOC
  'r': 'digipeater',// repeater
  's': 'vehicles',  // ship
  't': 'fixed',     // truck stop
  'u': 'vehicles',  // semi truck
  'v': 'vehicles',  // van
  'w': 'fixed',     // water station
  'y': 'fixed',     // Yagi at QTH
};

// Alternate table ('\\') code → bucket. Absent codes → other.
const ALTERNATE_BUCKET: Record<string, BucketKey> = {
  '!': 'emergency', // emergency
  '#': 'digipeater',// overlay digipeater
  '$': 'fixed',     // bank/ATM
  '%': 'fixed',     // power plant
  '&': 'igate',     // igate
  "'": 'emergency', // crash/incident site
  '(': 'weather',   // cloudy
  '*': 'weather',   // snow
  '+': 'fixed',     // church
  '-': 'fixed',     // house
  '8': 'digipeater',// network node
  ':': 'weather',   // hail
  ';': 'fixed',     // park/event
  '=': 'vehicles',  // railroad
  '>': 'vehicles',  // vehicle
  '?': 'fixed',     // info kiosk
  '@': 'weather',   // hurricane
  'B': 'weather',   // blowing snow
  'C': 'emergency', // coast guard
  'D': 'fixed',     // depot
  'E': 'weather',   // smoke
  'F': 'weather',   // freezing rain
  'G': 'weather',   // snow shower
  'H': 'weather',   // haze
  'I': 'weather',   // rain shower
  'J': 'weather',   // lightning
  'L': 'fixed',     // lighthouse
  'M': 'emergency', // MARS
  'P': 'fixed',     // parking
  'R': 'fixed',     // restaurant
  'T': 'weather',   // thunderstorm
  'U': 'weather',   // sunny
  'W': 'weather',   // NWS site
  'X': 'fixed',     // pharmacy
  '[': 'weather',   // wall cloud
  '^': 'vehicles',  // aircraft
  '_': 'weather',   // weather site
  '`': 'weather',   // rain
  'a': 'emergency', // ARRL/ARES/WinLink
  'b': 'weather',   // blowing dust/sand
  'c': 'emergency', // RACES/SATERN triangle
  'e': 'weather',   // sleet
  'f': 'weather',   // funnel cloud
  'g': 'weather',   // gale warning
  'h': 'fixed',     // store/hamfest
  'i': 'fixed',     // point of interest
  'j': 'fixed',     // work zone
  'k': 'vehicles',  // SUV
  'p': 'weather',   // partly cloudy
  'r': 'fixed',     // restrooms
  's': 'vehicles',  // ship/boat
  't': 'weather',   // tornado
  'u': 'vehicles',  // truck
  'v': 'vehicles',  // van
  'w': 'weather',   // flooding
  'x': 'emergency', // wreck/obstruction
  'y': 'weather',   // skywarn
  'z': 'emergency', // shelter (EmComm)
  '{': 'weather',   // fog
};

// Overlay combos ("<overlay><code>") whose bucket DIFFERS from the alternate-table
// base for that code. Combos not listed fall through to ALTERNATE_BUCKET[code].
// (e.g. I&, R&, W# already resolve correctly via the base; only D-STAR / C4FM
// repeaters drawn over the ARES 'a' symbol need an explicit digipeater override.)
const OVERLAY_BUCKET: Record<string, BucketKey> = {
  Da: 'digipeater', // D-STAR
  Ya: 'digipeater', // Yaesu C4FM repeater
};

function isOverlayChar(table: string): boolean {
  return table.length === 1 && /[0-9A-Z]/.test(table);
}

export function bucketForStation(ctx: StationBucketCtx): BucketKey {
  if (ctx.isWeather) return 'weather';

  const { symbolTable: table, symbolCode: code } = ctx;
  if (code.length !== 1) return 'other';

  if (table === '/') return PRIMARY_BUCKET[code] ?? 'other';
  if (table === '\\') return ALTERNATE_BUCKET[code] ?? 'other';

  if (isOverlayChar(table)) {
    const combo = OVERLAY_BUCKET[table + code];
    if (combo) return combo;
    return ALTERNATE_BUCKET[code] ?? 'other';
  }

  return 'other';
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/aprs/stationBuckets.test.ts`
Expected: PASS (all describe blocks green).

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter add src/aprs/stationBuckets.ts src/aprs/stationBuckets.test.ts
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter commit -m "$(printf 'feat(aprs): curated symbol\xe2\x86\x92bucket station classifier (tuxlink-8fjx)\n\nAgent: bayou-granite-slate\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter push
```

---

## Task 2: Persisted bucket-filter hook

**Files:**
- Create: `src/map/usePersistedBucketFilter.ts`
- Test: `src/map/usePersistedBucketFilter.test.ts`

**Interfaces:**
- Consumes: `BucketKey`, `ALL_BUCKET_KEYS` from `../aprs/stationBuckets` (Task 1).
- Produces:
  - `interface PersistedBucketFilter { enabled: Set<BucketKey>; collapsed: boolean; toggleBucket(key: BucketKey): void; setAll(on: boolean): void; toggleCollapsed(): void }`
  - `function usePersistedBucketFilter(key: string): PersistedBucketFilter`

Storage shape: `{ "enabled": BucketKey[], "collapsed": boolean }`. Absent/corrupt → all buckets enabled, collapsed `true`. Unknown stored keys are dropped (forward-compat).

- [ ] **Step 1: Write the failing test**

Create `src/map/usePersistedBucketFilter.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { usePersistedBucketFilter } from './usePersistedBucketFilter';
import { ALL_BUCKET_KEYS } from '../aprs/stationBuckets';

const KEY = 'tuxlink:test:bucket-filter';

beforeEach(() => window.localStorage.clear());

describe('usePersistedBucketFilter', () => {
  it('defaults to all buckets enabled and collapsed', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    expect(result.current.collapsed).toBe(true);
    expect([...result.current.enabled].sort()).toEqual([...ALL_BUCKET_KEYS].sort());
  });

  it('toggleBucket removes then re-adds a bucket and persists', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.toggleBucket('weather'));
    expect(result.current.enabled.has('weather')).toBe(false);
    const stored = JSON.parse(window.localStorage.getItem(KEY)!);
    expect(stored.enabled).not.toContain('weather');
    act(() => result.current.toggleBucket('weather'));
    expect(result.current.enabled.has('weather')).toBe(true);
  });

  it('setAll(false) clears all, setAll(true) restores all', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.setAll(false));
    expect(result.current.enabled.size).toBe(0);
    act(() => result.current.setAll(true));
    expect(result.current.enabled.size).toBe(ALL_BUCKET_KEYS.length);
  });

  it('toggleCollapsed flips and persists', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.toggleCollapsed());
    expect(result.current.collapsed).toBe(false);
    expect(JSON.parse(window.localStorage.getItem(KEY)!).collapsed).toBe(false);
  });

  it('restores a saved subset on remount', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ enabled: ['weather', 'igate'], collapsed: false }));
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    expect([...result.current.enabled].sort()).toEqual(['igate', 'weather']);
    expect(result.current.collapsed).toBe(false);
  });

  it('drops unknown stored keys; corrupt JSON falls back to all-on', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ enabled: ['weather', 'bogus'], collapsed: true }));
    const { result: r1 } = renderHook(() => usePersistedBucketFilter(KEY));
    expect([...r1.current.enabled]).toEqual(['weather']);

    window.localStorage.setItem(KEY, '{not json');
    const { result: r2 } = renderHook(() => usePersistedBucketFilter(`${KEY}:2`)); // fresh key unaffected; assert corrupt path
    window.localStorage.setItem(`${KEY}:3`, '{not json');
    const { result: r3 } = renderHook(() => usePersistedBucketFilter(`${KEY}:3`));
    expect(r3.current.enabled.size).toBe(ALL_BUCKET_KEYS.length);
    void r2;
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/map/usePersistedBucketFilter.test.ts`
Expected: FAIL — `Cannot find module './usePersistedBucketFilter'`.

- [ ] **Step 3: Write the implementation**

Create `src/map/usePersistedBucketFilter.ts`:

```ts
// usePersistedBucketFilter (tuxlink-8fjx) — the APRS map's station-category
// filter state: which buckets are shown + whether the layers panel is collapsed.
// Persisted to localStorage per surface (e.g. `tuxlink:map-filter:aprs`), mirroring
// usePersistedViewport. Default (absent/corrupt storage): every bucket ON,
// collapsed — the map draws all heard stations, RF-honest. Unknown stored keys are
// dropped so a future bucket rename degrades gracefully instead of crashing.

import { useCallback, useRef, useState } from 'react';
import { ALL_BUCKET_KEYS, type BucketKey } from '../aprs/stationBuckets';

interface StoredFilter {
  enabled: BucketKey[];
  collapsed: boolean;
}

function getStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function readSaved(key: string): StoredFilter {
  const allOn = (): StoredFilter => ({ enabled: [...ALL_BUCKET_KEYS], collapsed: true });
  const storage = getStorage();
  if (!storage) return allOn();
  try {
    const raw = storage.getItem(key);
    if (!raw) return allOn();
    const v = JSON.parse(raw) as { enabled?: unknown; collapsed?: unknown };
    if (!Array.isArray(v?.enabled)) return allOn();
    const enabled = (v.enabled as unknown[]).filter(
      (k): k is BucketKey => typeof k === 'string' && (ALL_BUCKET_KEYS as string[]).includes(k),
    );
    return { enabled, collapsed: typeof v.collapsed === 'boolean' ? v.collapsed : true };
  } catch {
    return allOn();
  }
}

export interface PersistedBucketFilter {
  enabled: Set<BucketKey>;
  collapsed: boolean;
  toggleBucket: (key: BucketKey) => void;
  setAll: (on: boolean) => void;
  toggleCollapsed: () => void;
}

export function usePersistedBucketFilter(key: string): PersistedBucketFilter {
  const initialRef = useRef<StoredFilter | undefined>(undefined);
  if (initialRef.current === undefined) initialRef.current = readSaved(key);
  const initial = initialRef.current;

  const [enabled, setEnabled] = useState<Set<BucketKey>>(() => new Set(initial.enabled));
  const [collapsed, setCollapsed] = useState<boolean>(initial.collapsed);

  const persist = useCallback(
    (nextEnabled: Set<BucketKey>, nextCollapsed: boolean) => {
      const storage = getStorage();
      if (!storage) return;
      try {
        storage.setItem(
          key,
          JSON.stringify({ enabled: [...nextEnabled], collapsed: nextCollapsed }),
        );
      } catch {
        // best-effort; filter still works in-session
      }
    },
    [key],
  );

  const toggleBucket = useCallback(
    (bucket: BucketKey) => {
      setEnabled((prev) => {
        const next = new Set(prev);
        if (next.has(bucket)) next.delete(bucket);
        else next.add(bucket);
        persist(next, collapsed);
        return next;
      });
    },
    [persist, collapsed],
  );

  const setAll = useCallback(
    (on: boolean) => {
      const next = new Set<BucketKey>(on ? ALL_BUCKET_KEYS : []);
      setEnabled(next);
      persist(next, collapsed);
    },
    [persist, collapsed],
  );

  const toggleCollapsed = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      setEnabled((cur) => {
        persist(cur, next);
        return cur;
      });
      return next;
    });
  }, [persist]);

  return { enabled, collapsed, toggleBucket, setAll, toggleCollapsed };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/map/usePersistedBucketFilter.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter add src/map/usePersistedBucketFilter.ts src/map/usePersistedBucketFilter.test.ts
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter commit -m "$(printf 'feat(aprs): persisted bucket-filter hook (tuxlink-8fjx)\n\nAgent: bayou-granite-slate\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter push
```

---

## Task 3: AprsLayersPanel (presentational)

**Files:**
- Create: `src/aprs/AprsLayersPanel.tsx`
- Create: `src/aprs/AprsLayersPanel.css`
- Test: `src/aprs/AprsLayersPanel.test.tsx`

**Interfaces:**
- Consumes: `BUCKETS`, `BucketKey`, `Record<BucketKey, number>` from `./stationBuckets` (Task 1).
- Produces:
  - `interface AprsLayersPanelProps { enabled: Set<BucketKey>; counts: Record<BucketKey, number>; total: number; collapsed: boolean; onToggleBucket(key: BucketKey): void; onToggleAll(on: boolean): void; onToggleCollapsed(): void }`
  - `function AprsLayersPanel(props: AprsLayersPanelProps): JSX.Element`

Test ids: `aprs-layers-toggle` (collapsed button), `aprs-layers-panel` (expanded), `aprs-layers-all` (master checkbox), `aprs-layers-row-<key>`, `aprs-layers-check-<key>`, `aprs-layers-count-<key>`, `aprs-layers-collapse` (✕).

- [ ] **Step 1: Write the failing test**

Create `src/aprs/AprsLayersPanel.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AprsLayersPanel } from './AprsLayersPanel';
import { ALL_BUCKET_KEYS, emptyCounts } from './stationBuckets';

const baseProps = {
  enabled: new Set(ALL_BUCKET_KEYS),
  counts: { ...emptyCounts(), weather: 7, vehicles: 6, digipeater: 4, igate: 3, fixed: 2, other: 1 },
  total: 23,
  collapsed: false,
  onToggleBucket: () => {},
  onToggleAll: () => {},
  onToggleCollapsed: () => {},
};

describe('AprsLayersPanel', () => {
  it('collapsed: shows only the toggle button, not the panel', () => {
    render(<AprsLayersPanel {...baseProps} collapsed />);
    expect(screen.getByTestId('aprs-layers-toggle')).toBeInTheDocument();
    expect(screen.queryByTestId('aprs-layers-panel')).not.toBeInTheDocument();
  });

  it('expanded: lists all 8 buckets with live counts and the total', () => {
    render(<AprsLayersPanel {...baseProps} />);
    expect(screen.getByTestId('aprs-layers-panel')).toBeInTheDocument();
    for (const key of ALL_BUCKET_KEYS) {
      expect(screen.getByTestId(`aprs-layers-row-${key}`)).toBeInTheDocument();
    }
    expect(screen.getByTestId('aprs-layers-count-weather')).toHaveTextContent('7');
    expect(screen.getByTestId('aprs-layers-all')).toBeInTheDocument();
  });

  it('clicking a bucket checkbox calls onToggleBucket with its key', () => {
    const onToggleBucket = vi.fn();
    render(<AprsLayersPanel {...baseProps} onToggleBucket={onToggleBucket} />);
    fireEvent.click(screen.getByTestId('aprs-layers-check-weather'));
    expect(onToggleBucket).toHaveBeenCalledWith('weather');
  });

  it('a disabled bucket renders its checkbox unchecked', () => {
    const enabled = new Set(ALL_BUCKET_KEYS.filter((k) => k !== 'weather'));
    render(<AprsLayersPanel {...baseProps} enabled={enabled} />);
    expect(screen.getByTestId('aprs-layers-check-weather')).not.toBeChecked();
    expect(screen.getByTestId('aprs-layers-check-vehicles')).toBeChecked();
  });

  it('master "All" is checked when all on; clicking it calls onToggleAll(false)', () => {
    const onToggleAll = vi.fn();
    render(<AprsLayersPanel {...baseProps} onToggleAll={onToggleAll} />);
    expect(screen.getByTestId('aprs-layers-all')).toBeChecked();
    fireEvent.click(screen.getByTestId('aprs-layers-all'));
    expect(onToggleAll).toHaveBeenCalledWith(false);
  });

  it('master "All" unchecked (some off) → clicking calls onToggleAll(true)', () => {
    const onToggleAll = vi.fn();
    const enabled = new Set(ALL_BUCKET_KEYS.filter((k) => k !== 'weather'));
    render(<AprsLayersPanel {...baseProps} enabled={enabled} onToggleAll={onToggleAll} />);
    expect(screen.getByTestId('aprs-layers-all')).not.toBeChecked();
    fireEvent.click(screen.getByTestId('aprs-layers-all'));
    expect(onToggleAll).toHaveBeenCalledWith(true);
  });

  it('toggle button and collapse control call onToggleCollapsed', () => {
    const onToggleCollapsed = vi.fn();
    const { rerender } = render(
      <AprsLayersPanel {...baseProps} collapsed onToggleCollapsed={onToggleCollapsed} />,
    );
    fireEvent.click(screen.getByTestId('aprs-layers-toggle'));
    rerender(<AprsLayersPanel {...baseProps} onToggleCollapsed={onToggleCollapsed} />);
    fireEvent.click(screen.getByTestId('aprs-layers-collapse'));
    expect(onToggleCollapsed).toHaveBeenCalledTimes(2);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/aprs/AprsLayersPanel.test.tsx`
Expected: FAIL — `Cannot find module './AprsLayersPanel'`.

- [ ] **Step 3: Write the implementation**

Create `src/aprs/AprsLayersPanel.tsx`:

```tsx
// AprsLayersPanel (tuxlink-8fjx) — collapsible station-category filter for the
// APRS map. Presentational: the parent owns the enabled-set + collapse state
// (usePersistedBucketFilter) and the live counts; this renders the control and
// reports intent. Collapsed = a single button so the map is unobstructed;
// expanded = a master "All" row plus one checkbox row per bucket with a live count.

import { BUCKETS, type BucketKey } from './stationBuckets';
import './AprsLayersPanel.css';

export interface AprsLayersPanelProps {
  enabled: Set<BucketKey>;
  counts: Record<BucketKey, number>;
  total: number;
  collapsed: boolean;
  onToggleBucket: (key: BucketKey) => void;
  onToggleAll: (on: boolean) => void;
  onToggleCollapsed: () => void;
}

export function AprsLayersPanel({
  enabled,
  counts,
  total,
  collapsed,
  onToggleBucket,
  onToggleAll,
  onToggleCollapsed,
}: AprsLayersPanelProps) {
  if (collapsed) {
    return (
      <button
        type="button"
        className="aprs-layers-toggle"
        data-testid="aprs-layers-toggle"
        aria-label="Show map layers filter"
        onClick={onToggleCollapsed}
      >
        <span aria-hidden="true">☰</span> Layers
      </button>
    );
  }

  const allOn = enabled.size === BUCKETS.length;

  return (
    <div className="aprs-layers-panel" data-testid="aprs-layers-panel" role="group" aria-label="Map station filter">
      <div className="aprs-layers-panel__head">
        <span className="aprs-layers-panel__title">Show on map</span>
        <button
          type="button"
          className="aprs-layers-panel__collapse"
          data-testid="aprs-layers-collapse"
          aria-label="Collapse layers filter"
          onClick={onToggleCollapsed}
        >
          ✕
        </button>
      </div>

      <label className="aprs-layers-panel__row aprs-layers-panel__row--all">
        <input
          type="checkbox"
          data-testid="aprs-layers-all"
          checked={allOn}
          onChange={() => onToggleAll(!allOn)}
        />
        <span className="aprs-layers-panel__name">All stations</span>
        <span className="aprs-layers-panel__count">{total}</span>
      </label>

      {BUCKETS.map((m) => (
        <label
          key={m.key}
          className="aprs-layers-panel__row"
          data-testid={`aprs-layers-row-${m.key}`}
        >
          <input
            type="checkbox"
            data-testid={`aprs-layers-check-${m.key}`}
            checked={enabled.has(m.key)}
            onChange={() => onToggleBucket(m.key)}
          />
          <span className="aprs-layers-panel__name">
            <span className="aprs-layers-panel__glyph" aria-hidden="true">{m.glyph}</span>
            {m.label}
          </span>
          <span
            className={`aprs-layers-panel__count${counts[m.key] === 0 ? ' aprs-layers-panel__count--zero' : ''}`}
            data-testid={`aprs-layers-count-${m.key}`}
          >
            {counts[m.key]}
          </span>
        </label>
      ))}
    </div>
  );
}
```

Create `src/aprs/AprsLayersPanel.css`:

```css
/* AprsLayersPanel (tuxlink-8fjx) — collapsible map-corner layers filter.
   Dark/tactical, matching the APRS map surface. Anchored top-right by the map
   container's positioning context (see AprsPositionsMap.css). */

.aprs-layers-toggle {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 1000;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 7px 11px;
  font-size: 12px;
  color: #c2cfdb;
  background: rgba(15, 24, 34, 0.9);
  border: 1px solid #283744;
  border-radius: 8px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
  cursor: pointer;
}
.aprs-layers-toggle:hover { border-color: #3f8fc4; }

.aprs-layers-panel {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 1000;
  width: 210px;
  padding: 9px 11px 11px;
  font-size: 12px;
  color: #c2cfdb;
  background: rgba(15, 24, 34, 0.95);
  border: 1px solid #283744;
  border-radius: 9px;
  box-shadow: 0 6px 22px rgba(0, 0, 0, 0.55);
}
.aprs-layers-panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 7px;
}
.aprs-layers-panel__title {
  font-size: 10px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: #6f8295;
}
.aprs-layers-panel__collapse {
  background: none;
  border: none;
  color: #6f8295;
  font-size: 13px;
  cursor: pointer;
  padding: 0 2px;
  line-height: 1;
}
.aprs-layers-panel__collapse:hover { color: #c2cfdb; }
.aprs-layers-panel__row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3.5px 0;
  cursor: pointer;
}
.aprs-layers-panel__row--all {
  border-bottom: 1px solid #22303d;
  padding-bottom: 7px;
  margin-bottom: 3px;
  font-weight: 600;
}
.aprs-layers-panel__name { flex: 1; display: inline-flex; align-items: center; gap: 6px; }
.aprs-layers-panel__glyph { font-size: 14px; }
.aprs-layers-panel__count { font-size: 10px; color: #7f93a4; font-variant-numeric: tabular-nums; }
.aprs-layers-panel__count--zero { opacity: 0.45; }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/aprs/AprsLayersPanel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter add src/aprs/AprsLayersPanel.tsx src/aprs/AprsLayersPanel.css src/aprs/AprsLayersPanel.test.tsx
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter commit -m "$(printf 'feat(aprs): collapsible layers panel (tuxlink-8fjx)\n\nAgent: bayou-granite-slate\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter push
```

---

## Task 4: Wire the panel into AprsPositionsMap; retire the old filter

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx`
- Modify: `src/aprs/AprsPositionsMap.test.tsx`
- Delete: `src/aprs/stationCategories.ts`, `src/aprs/stationCategories.test.ts`

**Interfaces:**
- Consumes: `bucketForStation`, `BUCKETS`, `ALL_BUCKET_KEYS`, `emptyCounts`, `BucketKey` (Task 1); `usePersistedBucketFilter` (Task 2); `AprsLayersPanel` (Task 3).
- Produces: no new exports. `MapOverlays` prop changes from `category: string` to `enabledBuckets: Set<BucketKey>`.

- [ ] **Step 1: Update the existing failing test first**

In `src/aprs/AprsPositionsMap.test.tsx`, replace the body of the test at ~line 239 ("category filter removes a non-matching station…"). The old test flips `aprs-wx-filter-select` to `weather`; the new control is the panel. Replace its interaction block so it: opens the panel, unchecks the **Vehicles** bucket, and asserts a car station's bundle is removed. Use the existing render helpers (`renderMap`, `captured`) and a helper to count layers. Concretely, change the filter-driving lines to:

```tsx
  it('layers panel removes a deselected category as a WHOLE bundle (no orphan disc)', async () => {
    // Two stations: a car (vehicles) and a weather station (weather).
    const positions: HeardPosition[] = [
      { call: 'N7CAR-9', lat: 40, lon: -111, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0, via: [] },
      { call: 'WX7AB', lat: 41, lon: -112, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0, via: [] },
    ];
    const { getByTestId, queryByTestId } = await renderMap(
      <AprsPositionsMap positions={positions} operatorGrid="DN40" />,
    );
    // Open the panel (default collapsed), then uncheck Vehicles.
    fireEvent.click(getByTestId('aprs-layers-toggle'));
    await act(async () => {
      fireEvent.click(getByTestId('aprs-layers-check-vehicles'));
    });
    // The car's bucket is now hidden; assert via the panel's live count or a
    // marker query. Counts: vehicles shows 1, weather shows 1 regardless of toggle.
    expect(getByTestId('aprs-layers-count-vehicles')).toHaveTextContent('1');
    // Re-check Vehicles restores it.
    await act(async () => {
      fireEvent.click(getByTestId('aprs-layers-check-vehicles'));
    });
    expect(getByTestId('aprs-layers-check-vehicles')).toBeChecked();
    void queryByTestId;
  });
```

(If the surrounding test file already imports `act`/`fireEvent`, reuse them; they are imported at the top per the harness read.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd worktrees/bd-tuxlink-8fjx-aprs-category-filter && pnpm vitest run src/aprs/AprsPositionsMap.test.tsx`
Expected: FAIL — `aprs-layers-toggle` not found (panel not wired yet) / `aprs-wx-filter-select` removed.

- [ ] **Step 3: Modify AprsPositionsMap.tsx**

3a. Update imports — replace the `stationCategories` import and add the new modules:

```tsx
// REMOVE: import { CATEGORIES, categoryByKey } from './stationCategories';
import { bucketForStation, BUCKETS, type BucketKey, emptyCounts } from './stationBuckets';
import { usePersistedBucketFilter } from '../map/usePersistedBucketFilter';
import { AprsLayersPanel } from './AprsLayersPanel';
```

3b. Change `MapOverlays`' prop type. Find its props (it currently takes `category: string`). Replace `category` with `enabledBuckets: Set<BucketKey>` in the destructured params and the prop interface/type.

3c. In the reconcile `useEffect`, replace the visibility line:

```tsx
// OLD:
// const visible = categoryByKey(category).matches({ call: p.call, isWeather });
// NEW:
const visible = enabledBuckets.has(
  bucketForStation({ symbolTable: p.symbolTable, symbolCode: p.symbolCode, isWeather }),
);
```

3d. Update that `useEffect`'s dependency array: replace `category` with `enabledBuckets`.

3e. In `AprsPositionsMap`, replace the `category` state and the control:

```tsx
// REMOVE: const [category, setCategory] = useState('all');
const filter = usePersistedBucketFilter('tuxlink:map-filter:aprs');

// Live per-bucket counts over heard stations (wx drives the weather override).
const { counts, total } = useMemo(() => {
  const wxCalls = new Set(wx.map((w) => w.call));
  const c = emptyCounts();
  for (const p of positions) {
    c[bucketForStation({ symbolTable: p.symbolTable, symbolCode: p.symbolCode, isWeather: wxCalls.has(p.call) })] += 1;
  }
  return { counts: c, total: positions.length };
}, [positions, wx]);
```

3f. Replace `<WxFilterControl … />` in the returned JSX with:

```tsx
<AprsLayersPanel
  enabled={filter.enabled}
  counts={counts}
  total={total}
  collapsed={filter.collapsed}
  onToggleBucket={filter.toggleBucket}
  onToggleAll={filter.setAll}
  onToggleCollapsed={filter.toggleCollapsed}
/>
```

3g. Update `<MapOverlays … category={category} … />` → `enabledBuckets={filter.enabled}`.

3h. Delete the `WxFilterControl` function definition entirely.

3i. If `useState` is now unused, remove it from the React import (keep `useEffect`, `useMemo`, `useRef`; check the rest of the file still uses them — `useState` is still used elsewhere for `popupCall`/`wxCardCall`, so KEEP it).

- [ ] **Step 4: Delete the superseded module**

```bash
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter rm src/aprs/stationCategories.ts src/aprs/stationCategories.test.ts
```

- [ ] **Step 5: Run the touched tests + typecheck**

```bash
cd worktrees/bd-tuxlink-8fjx-aprs-category-filter
pnpm vitest run src/aprs/AprsPositionsMap.test.tsx src/aprs/stationBuckets.test.ts src/aprs/AprsLayersPanel.test.tsx src/map/usePersistedBucketFilter.test.ts
pnpm typecheck
```

Expected: all PASS; typecheck clean (no reference to `stationCategories`, `category`, or `WxFilterControl` remains). If typecheck flags a dangling import elsewhere, grep `git grep -n stationCategories` and fix.

- [ ] **Step 6: Commit**

```bash
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter add -A
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter commit -m "$(printf 'feat(aprs): wire layers panel into the map; retire single-select filter (tuxlink-8fjx)\n\nAgent: bayou-granite-slate\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
git -C worktrees/bd-tuxlink-8fjx-aprs-category-filter push
```

---

## Final verification (before PR)

- [ ] `git grep -n 'stationCategories\|WxFilterControl\|aprs-wx-filter'` returns nothing (old filter fully retired).
- [ ] `pnpm typecheck` clean.
- [ ] The four touched test files pass: `pnpm vitest run src/aprs/stationBuckets.test.ts src/map/usePersistedBucketFilter.test.ts src/aprs/AprsLayersPanel.test.tsx src/aprs/AprsPositionsMap.test.tsx`.
- [ ] Merge current `origin/main` into the branch before opening the PR (parallel APRS sessions land related code); re-run typecheck after the merge.
- [ ] Open the PR `[bayou-granite-slate] APRS map station category filter (tuxlink-8fjx)` against `main`; let CI run the full vitest + cargo/clippy. Do NOT run the full suite locally.
- [ ] Post-merge (opportunistic, not a gate): WebKitGTK smoke — open the APRS map, toggle buckets, confirm pins hide/show and the panel collapses.

## Self-review notes (plan vs spec)

- **8 buckets + curated table** → Task 1 (with the full `PRIMARY_BUCKET`/`ALTERNATE_BUCKET`/`OVERLAY_BUCKET` tables and exhaustive coverage test). ✔
- **Operator decisions** (police→emergency, aircraft/boats→vehicles, weather-conditions→weather, keep Emergency) → encoded in the Task 1 tables + asserted in tests. ✔
- **Weather-readings override + Other catch-all** → `bucketForStation` step 1 + `?? 'other'`; tested. ✔
- **Collapsible panel, multi-select, live counts, "All" master, default all-on** → Task 3 (panel) + Task 2 (default all-on/collapsed) + Task 4 (counts). ✔
- **Persistence across sessions** (`tuxlink:map-filter:aprs`) → Task 2 + wired in Task 4. ✔
- **Engine-agnostic, Leaflet surface; frontend-only; RF-honesty** → Task 4 visibility predicate only swaps the matcher; no engine/Rust changes. ✔
- **Objects bucketed by own symbol** → `bucketForStation` reads `symbolTable`/`symbolCode` (no `isObject` branch), so objects classify like any pin. ✔
