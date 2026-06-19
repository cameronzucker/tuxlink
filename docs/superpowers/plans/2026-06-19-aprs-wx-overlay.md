# APRS on-map weather overlay + category filter (ni5b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render heard APRS weather stations with their real readings as a temperature-led badge (hover → full card, click → Station Data), behind a generic station-category filter ("weather mode"), with an optional PNG map snapshot.

**Architecture:** Pure join of `useEnvStations` (WX channels by call) ⋈ `useAprsPositions` (lat/lon by call) → weather stations; a maplibre symbol badge layer + a category-predicate filter on the existing positions map; click threads `onFocusStation` to AppShell which focuses the existing Station Data card; an optional canvas PNG export. No backend change — the WX + position event seams already exist.

**Tech Stack:** React 18 + TypeScript, maplibre-gl v5, vitest.

## Global Constraints

- Serde wire forms are camelCase; no backend/DTO change in this feature.
- RF-honesty: the badge carries only what was heard. Temperature-led; a condition glyph ONLY when a real field supports it (`🌧` when rain1h > 0; a wind glyph when wind ≥ 20 mph); never an assumed ☀. Null fields are omitted, never fabricated.
- No heatmap / interpolation (rejected — fabricates data).
- Frontend tests run locally: `pnpm vitest run <file>`. Typecheck: `pnpm typecheck`.
- This Pi can't finish a cold cargo build — irrelevant here (no Rust), but `pnpm build` (vite) is the prod gate.
- Commit trailer every commit: `Agent: mink-yew-osprey` + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Worktree cwd gotcha: run a standalone `cd <worktree>` Bash call before any `git` op.
- v1 scope: map overlay + WX-only filter (generic mechanism) + optional PNG. Deferred: hepq (Winlink text report), tuxlink-8fjx (other categories).

## Data source note

`useEnvStations` does NOT expose the raw `WeatherReportDto`. It exposes `EnvStation` with `channels: EnvChannel[]` (weather channels keyed `wx:<kind>`, e.g. `kind: 'temperature'`, value °F; `kind: 'wind_speed'`, value mph) and `rain: RainTotals | null` (`in1h`/`in24h`/`sinceMidnight`). The badge reads from these, not the DTO.

---

### Task 1: `wxStations.ts` — join + classification + honest badge content

**Files:**
- Create: `src/aprs/wxStations.ts`
- Test: `src/aprs/wxStations.test.ts`

**Interfaces:**
- Consumes: `EnvStation`, `EnvChannel`, `RainTotals` from `./envStations`; `HeardPosition` from `./aprsTypes`.
- Produces: `hasWeather(env: EnvStation): boolean`; `WxStation { call, lat, lon, env, at }`; `joinWxStations(env: EnvStation[], positions: HeardPosition[]): WxStation[]`; `badgeContent(env: EnvStation): { primary: string; glyph: string | null }`.

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, it, expect } from 'vitest';
import { hasWeather, joinWxStations, badgeContent } from './wxStations';
import type { EnvStation } from './envStations';
import type { HeardPosition } from './aprsTypes';

function env(call: string, ch: Array<{ key: string; kind: string; value: number; unit?: string }>, rain: EnvStation['rain'] = null): EnvStation {
  return {
    call, project: '', seq: null, bits: [], rain, lastHeard: 100,
    channels: ch.map((c) => ({ key: c.key, label: c.kind, unit: c.unit ?? '', kind: c.kind as never, value: c.value, scaled: true, history: [] })),
  };
}
function pos(call: string, lat: number, lon: number): HeardPosition {
  return { call, lat, lon, symbolTable: '/', symbolCode: '_', comment: '', at: 1, ambiguity: 0 };
}

describe('hasWeather', () => {
  it('true for a wx channel', () => {
    expect(hasWeather(env('W7WX', [{ key: 'wx:temperature', kind: 'temperature', value: 72 }]))).toBe(true);
  });
  it('true for rain totals only', () => {
    expect(hasWeather(env('W7WX', [], { in1h: 0.1, in24h: null, sinceMidnight: null }))).toBe(true);
  });
  it('false for telemetry-only station', () => {
    expect(hasWeather(env('N0T', [{ key: 'tlm:Vbat', kind: 'generic', value: 13 }]))).toBe(false);
  });
});

describe('joinWxStations', () => {
  it('includes only weather stations that also have a position', () => {
    const stations = [
      env('W7WX', [{ key: 'wx:temperature', kind: 'temperature', value: 72 }]),
      env('NOPOS', [{ key: 'wx:temperature', kind: 'temperature', value: 60 }]),
      env('N0T', [{ key: 'tlm:Vbat', kind: 'generic', value: 13 }]),
    ];
    const positions = [pos('W7WX', 47, -122), pos('N0T', 40, -100)];
    const out = joinWxStations(stations, positions);
    expect(out.map((w) => w.call)).toEqual(['W7WX']); // NOPOS has no position; N0T has no weather
    expect(out[0].lat).toBe(47);
  });
});

describe('badgeContent (RF-honesty)', () => {
  it('temperature-led, no glyph when only temp', () => {
    expect(badgeContent(env('W', [{ key: 'wx:temperature', kind: 'temperature', value: 71.6 }]))).toEqual({ primary: '72°F', glyph: null });
  });
  it('rain glyph only when actually raining', () => {
    const e = env('W', [{ key: 'wx:temperature', kind: 'temperature', value: 60 }], { in1h: 0.2, in24h: null, sinceMidnight: null });
    expect(badgeContent(e)).toEqual({ primary: '60°F', glyph: '🌧' });
  });
  it('wind glyph when wind is notable and no rain', () => {
    const e = env('W', [
      { key: 'wx:temperature', kind: 'temperature', value: 60 },
      { key: 'wx:wind_speed', kind: 'wind_speed', value: 22, unit: 'mph' },
    ]);
    expect(badgeContent(e)).toEqual({ primary: '60°F', glyph: '💨' });
  });
  it('falls back to wind reading when no temperature', () => {
    expect(badgeContent(env('W', [{ key: 'wx:wind_speed', kind: 'wind_speed', value: 12, unit: 'mph' }]))).toEqual({ primary: '12 mph', glyph: null });
  });
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/aprs/wxStations.test.ts` → FAIL (module not found).

- [ ] **Step 3: Implement `wxStations.ts`**

```ts
// src/aprs/wxStations.ts
//
// Pure join + classification for the on-map weather overlay (ni5b). Weather data
// lives in EnvStation.channels (keyed `wx:<kind>`) + EnvStation.rain; positions
// live in HeardPosition. A WxStation exists only when a station has BOTH a heard
// weather reading and a position. RF-honesty: the badge shows only what was heard
// — temperature-led, a condition glyph only when a real field supports it.

import type { EnvStation } from './envStations';
import type { HeardPosition } from './aprsTypes';

/// A station counts as "weather" when it emitted any weather channel or rain —
/// distinct from a telemetry-only station (channels keyed `tlm:`).
export function hasWeather(env: EnvStation): boolean {
  return env.rain != null || env.channels.some((c) => c.key.startsWith('wx:'));
}

export interface WxStation {
  call: string;
  lat: number;
  lon: number;
  env: EnvStation;
  /// Local epoch-ms of the latest frame (from EnvStation.lastHeard).
  at: number;
}

/// Inner-join weather stations with their positions by callsign. Excludes
/// telemetry-only stations and weather stations with no heard position.
export function joinWxStations(env: EnvStation[], positions: HeardPosition[]): WxStation[] {
  const posByCall = new Map(positions.map((p) => [p.call, p]));
  const out: WxStation[] = [];
  for (const e of env) {
    if (!hasWeather(e)) continue;
    const p = posByCall.get(e.call);
    if (!p) continue;
    out.push({ call: e.call, lat: p.lat, lon: p.lon, env: e, at: e.lastHeard });
  }
  return out;
}

/// The compact badge: a temperature-led primary string + an optional condition
/// glyph derived ONLY from real fields. Never assumes a sky condition.
export function badgeContent(env: EnvStation): { primary: string; glyph: string | null } {
  const temp = env.channels.find((c) => c.kind === 'temperature');
  const wind = env.channels.find((c) => c.kind === 'wind_speed');
  const rain1h = env.rain?.in1h ?? null;

  let primary: string;
  if (temp) primary = `${Math.round(temp.value)}°F`;
  else if (wind) primary = `${Math.round(wind.value)} mph`;
  else {
    const first = env.channels[0];
    primary = first ? `${Math.round(first.value)} ${first.unit}`.trim() : '—';
  }

  let glyph: string | null = null;
  if (rain1h != null && rain1h > 0) glyph = '🌧';
  else if (wind && wind.value >= 20) glyph = '💨';
  return { primary, glyph };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/aprs/wxStations.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ni5b-wx-overlay
git add src/aprs/wxStations.ts src/aprs/wxStations.test.ts
git commit -m "feat(aprs): wxStations join + honest badge content (ni5b)

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `stationCategories.ts` — generic category filter

**Files:**
- Create: `src/aprs/stationCategories.ts`
- Test: `src/aprs/stationCategories.test.ts`

**Interfaces:**
- Produces: `StationCategoryCtx { call: string; isWeather: boolean }`; `StationCategory { key: string; label: string; matches(ctx): boolean }`; `CATEGORIES: StationCategory[]`; `categoryByKey(key): StationCategory`.

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, it, expect } from 'vitest';
import { CATEGORIES, categoryByKey } from './stationCategories';

describe('stationCategories', () => {
  it('all matches everything', () => {
    const all = categoryByKey('all');
    expect(all.matches({ call: 'X', isWeather: false })).toBe(true);
  });
  it('weather matches only weather stations', () => {
    const wx = categoryByKey('weather');
    expect(wx.matches({ call: 'X', isWeather: true })).toBe(true);
    expect(wx.matches({ call: 'X', isWeather: false })).toBe(false);
  });
  it('exposes weather as a selectable category', () => {
    expect(CATEGORIES.map((c) => c.key)).toContain('weather');
  });
  it('unknown key falls back to all', () => {
    expect(categoryByKey('nope').key).toBe('all');
  });
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/aprs/stationCategories.test.ts` → FAIL.

- [ ] **Step 3: Implement**

```ts
// src/aprs/stationCategories.ts
//
// Generic station-category filter for the APRS map. "Weather mode" is the first
// category; tuxlink-8fjx extends this list (vehicles/digipeaters/iGates) by
// adding predicates — the filter mechanism does not change. Each category is a
// pure predicate over a small per-station context.

export interface StationCategoryCtx {
  call: string;
  isWeather: boolean;
}

export interface StationCategory {
  key: string;
  label: string;
  matches(ctx: StationCategoryCtx): boolean;
}

export const CATEGORIES: StationCategory[] = [
  { key: 'all', label: 'All stations', matches: () => true },
  { key: 'weather', label: 'Weather', matches: (ctx) => ctx.isWeather },
];

export function categoryByKey(key: string): StationCategory {
  return CATEGORIES.find((c) => c.key === key) ?? CATEGORIES[0];
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/aprs/stationCategories.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ni5b-wx-overlay
git add src/aprs/stationCategories.ts src/aprs/stationCategories.test.ts
git commit -m "feat(aprs): generic station-category filter (weather first) (ni5b)

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `wxSnapshot.ts` — SITREP header (pure)

**Files:**
- Create: `src/aprs/wxSnapshot.ts`
- Test: `src/aprs/wxSnapshot.test.ts`

**Interfaces:**
- Produces: `composeSnapshotHeader(meta: { grid?: string; utcMs: number; stationCount: number }): string`.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect } from 'vitest';
import { composeSnapshotHeader } from './wxSnapshot';

describe('composeSnapshotHeader', () => {
  it('includes grid, UTC, and station count', () => {
    // 2026-06-19T19:42:00Z
    const h = composeSnapshotHeader({ grid: 'DM43', utcMs: Date.UTC(2026, 5, 19, 19, 42, 0), stationCount: 7 });
    expect(h).toContain('DM43');
    expect(h).toContain('1942Z');
    expect(h).toContain('7');
  });
  it('omits the grid segment when no grid is known', () => {
    const h = composeSnapshotHeader({ utcMs: Date.UTC(2026, 5, 19, 1, 5, 0), stationCount: 0 });
    expect(h).not.toContain('grid');
    expect(h).toContain('0105Z');
  });
});
```

- [ ] **Step 2: Run to verify fail** → `pnpm vitest run src/aprs/wxSnapshot.test.ts` FAIL.

- [ ] **Step 3: Implement**

```ts
// src/aprs/wxSnapshot.ts
//
// Pure header text for the weather map snapshot (ni5b). The canvas compositing +
// PNG download is the imperative shell; this builds the burned-in header so it is
// unit-testable. Honest: the grid segment is omitted when no operator grid is set.

export function composeSnapshotHeader(meta: { grid?: string; utcMs: number; stationCount: number }): string {
  const d = new Date(meta.utcMs);
  const hh = String(d.getUTCHours()).padStart(2, '0');
  const mm = String(d.getUTCMinutes()).padStart(2, '0');
  const time = `${hh}${mm}Z`;
  const parts = ['Local WX'];
  if (meta.grid) parts.push(`grid ${meta.grid}`);
  parts.push(time);
  parts.push(`${meta.stationCount} stn`);
  return parts.join(' · ');
}
```

- [ ] **Step 4: Run to verify pass** → PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ni5b-wx-overlay
git add src/aprs/wxSnapshot.ts src/aprs/wxSnapshot.test.ts
git commit -m "feat(aprs): wx snapshot SITREP header (pure) (ni5b)

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Station Data focus seam — `EnvPanel` `focusCall`

**Files:**
- Modify: `src/aprs/EnvPanel.tsx` (props + scroll/highlight)
- Modify: `src/aprs/EnvPanel.css` (a transient highlight class)
- Test: `src/aprs/EnvPanel.test.tsx`

**Interfaces:**
- Produces: `EnvPanelProps.focusCall?: string | null` — when set, that station's card scrolls into view and is briefly highlighted.

- [ ] **Step 1: Write the failing test**

Add to `EnvPanel.test.tsx` (create if absent; follow the EnvStation fixture used in `envStations.test.ts`):

```ts
import { render } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { EnvPanel } from './EnvPanel';
import type { EnvStation } from './envStations';

function station(call: string): EnvStation {
  return { call, project: '', seq: null, channels: [], bits: [], rain: null, lastHeard: 1 };
}

describe('EnvPanel focusCall', () => {
  it('scrolls the focused station card into view', () => {
    const scroll = vi.fn();
    // jsdom has no scrollIntoView; stub it.
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { value: scroll, writable: true });
    const { getByText } = render(<EnvPanel stations={[station('W7AAA'), station('W7BBB')]} focusCall="W7BBB" now={1} />);
    expect(getByText('W7BBB')).toBeInTheDocument();
    expect(scroll).toHaveBeenCalled();
  });
});
```

(If `EnvStationCard` does not render the callsign as plain text, assert on its testid instead — read `EnvStationCard.tsx` first.)

- [ ] **Step 2: Run to verify fail** → FAIL (no `focusCall`).

- [ ] **Step 3: Implement**

In `EnvPanel.tsx`:

```tsx
import { useEffect, useRef } from 'react';
import './EnvPanel.css';
import { EnvStationCard } from './EnvStationCard';
import type { EnvStation } from './envStations';

export interface EnvPanelProps {
  stations: EnvStation[];
  now?: number;
  /// When set, scroll this station's card into view + briefly highlight it.
  /// Threaded from a map WX-badge click (ni5b). Unset = unchanged behaviour.
  focusCall?: string | null;
}

export function EnvPanel({ stations, now = Date.now(), focusCall = null }: EnvPanelProps) {
  const focusRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (focusCall && focusRef.current) {
      focusRef.current.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }, [focusCall]);

  if (stations.length === 0) {
    // ... unchanged empty state ...
  }
  return (
    <div className="env-panel" data-testid="env-panel">
      <div className="env-list">
        {stations.map((s) => (
          <div
            key={s.call}
            ref={s.call === focusCall ? focusRef : undefined}
            className={s.call === focusCall ? 'env-card-focus' : undefined}
          >
            <EnvStationCard station={s} now={now} />
          </div>
        ))}
      </div>
    </div>
  );
}
```

In `EnvPanel.css` add a transient highlight:

```css
.env-card-focus { animation: env-focus-flash 1.6s ease-out 1; border-radius: 8px; }
@keyframes env-focus-flash { 0% { box-shadow: 0 0 0 2px #4ea1ff; } 100% { box-shadow: 0 0 0 2px transparent; } }
```

- [ ] **Step 4: Run to verify pass** → `pnpm vitest run src/aprs/EnvPanel.test.tsx` PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ni5b-wx-overlay
git add src/aprs/EnvPanel.tsx src/aprs/EnvPanel.css src/aprs/EnvPanel.test.tsx
git commit -m "feat(aprs): EnvPanel focusCall scroll+highlight seam (ni5b)

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Map badge layer + category filter + WX card + click

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx` (new `envStations` + `onFocusStation` props; badge layer; filter control; hover card; click → focus)
- Modify: `src/aprs/AprsPositionsMap.css` (WX card + filter control styling)
- Test: `src/aprs/AprsPositionsMap.test.tsx`

**Interfaces:**
- Consumes: `joinWxStations`, `badgeContent`, `WxStation` (Task 1); `CATEGORIES`, `categoryByKey` (Task 2).
- Produces: `AprsPositionsMapProps` gains `envStations?: EnvStation[]` and `onFocusStation?: (call: string) => void`.

- [ ] **Step 1: Write the failing test**

Extend `AprsPositionsMap.test.tsx` (uses the `getLastMap`/`loadLast` maplibre double):

```ts
it('renders a WX badge layer + feature for a heard weather station', () => {
  const positions: HeardPosition[] = [
    { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: 1, ambiguity: 0 },
  ];
  const envStations = [{
    call: 'W7WX', project: '', seq: null, bits: [], rain: null, lastHeard: 1,
    channels: [{ key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: 72, scaled: true, history: [] }],
  }];
  render(<AprsPositionsMap positions={positions} envStations={envStations as never} operatorGrid="CN87" />);
  const map = loadLast();
  expect(map.__state.layers.some((l) => l.id === 'aprs-wx-badge')).toBe(true);
  const feats = (map.getSource('aprs-wx-badge') as { data: { features: Array<{ properties: { badge: string } }> } }).data.features;
  expect(feats.length).toBe(1);
  expect(feats[0].properties.badge).toContain('72°F');
});

it('invokes onFocusStation when a WX badge pin is clicked', () => {
  const onFocus = vi.fn();
  const positions: HeardPosition[] = [
    { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: 1, ambiguity: 0 },
  ];
  const envStations = [{
    call: 'W7WX', project: '', seq: null, bits: [], rain: null, lastHeard: 1,
    channels: [{ key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: 72, scaled: true, history: [] }],
  }];
  render(<AprsPositionsMap positions={positions} envStations={envStations as never} onFocusStation={onFocus} operatorGrid="CN87" />);
  const map = loadLast();
  act(() => map.__emit('click:aprs-wx-badge', { features: [{ properties: { call: 'W7WX' } }] }));
  expect(onFocus).toHaveBeenCalledWith('W7WX');
});
```

- [ ] **Step 2: Run to verify fail** → FAIL (no `aprs-wx-badge`).

- [ ] **Step 3: Implement.** Add constants + a `wxBadgeFC` builder + a `WxBadgeLayer` child, mirroring the existing `PositionLayers`/`useMapOverlay` pattern. Add near the other source ids:

```ts
const WX_BADGE_SOURCE = 'aprs-wx-badge';
const WX_BADGE_LAYER = 'aprs-wx-badge';
const WX_BADGE_LAYERS = ([
  {
    id: WX_BADGE_LAYER, type: 'symbol', source: WX_BADGE_SOURCE,
    layout: {
      'text-field': ['get', 'badge'], 'text-size': 12, 'text-offset': [0, -1.6],
      'text-anchor': 'bottom', 'text-allow-overlap': true,
    },
    paint: { 'text-color': '#ffe0a3', 'text-halo-color': '#0c1620', 'text-halo-width': 1.4 },
  },
] as unknown[]).map((l) => l as Record<string, unknown> & { id: string });
```

```tsx
import { joinWxStations, badgeContent, type WxStation } from './wxStations';
import { categoryByKey } from './stationCategories';
import type { EnvStation } from './envStations';

function wxBadgeFC(wx: WxStation[]): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: wx.map((w) => {
      const b = badgeContent(w.env);
      return {
        type: 'Feature', id: w.call,
        properties: { call: w.call, badge: b.glyph ? `${b.primary} ${b.glyph}` : b.primary },
        geometry: { type: 'Point', coordinates: [w.lon, w.lat] },
      };
    }),
  };
}

function WxBadgeLayer({
  wx, onFocusStation,
}: { wx: WxStation[]; onFocusStation?: (call: string) => void }) {
  const map = useMapContext();
  useMapOverlay(map, WX_BADGE_SOURCE, { type: 'geojson', data: EMPTY_FC }, WX_BADGE_LAYERS);
  const fc = useMemo(() => wxBadgeFC(wx), [wx]);
  usePushData(map, WX_BADGE_SOURCE, fc);
  const onFocusRef = useRef(onFocusStation);
  onFocusRef.current = onFocusStation;
  useEffect(() => {
    if (!map) return;
    const onClick = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call != null) onFocusRef.current?.(String(call));
    };
    map.on('click', WX_BADGE_LAYER, onClick as (...a: unknown[]) => void);
    return () => map.off('click', WX_BADGE_LAYER, onClick as (...a: unknown[]) => void);
  }, [map]);
  return null;
}
```

Update props + render in `AprsPositionsMap`:

```tsx
export interface AprsPositionsMapProps {
  positions: HeardPosition[];
  operatorGrid?: string;
  envStations?: EnvStation[];
  onFocusStation?: (call: string) => void;
}
// inside the component, before <MapLibreMap>:
const wx = useMemo(() => joinWxStations(envStations ?? [], positions), [envStations, positions]);
// inside <MapLibreMap>, after <PositionLayers>:
<WxBadgeLayer wx={wx} onFocusStation={onFocusStation} />
```

Add the **category filter control** (`WxFilterControl`): a small select bound to a `category` state (default `'all'`); when `'weather'`, set a maplibre `filter` on the position pin layers to weather calls (compute the weather-call set from `wx`) and hide non-weather. (Apply via `map.setFilter(POSITION_PINS_COLOR_LAYER, ['in', ['get','call'], ['literal', weatherCalls]])` when weather mode; reset to the registered filter when `'all'`.) Render it as an HTML control in the map container (mirror `RecenterControl`). The hover **WX card (B)** reuses the existing pin `mouseenter`/`mouseleave` seam to show an HTML overlay listing `w.env.channels` + `rain` (only fields present).

- [ ] **Step 4: Run to verify pass + typecheck** → `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx && pnpm typecheck` PASS.

- [ ] **Step 5: Commit** (`feat(aprs): on-map WX badges + category filter + click→Station Data (ni5b)`).

---

### Task 6: Wire into AppShell + PNG export

**Files:**
- Modify: `src/shell/AppShell.tsx` (pass `envStations.stations` + `onFocusStation` to the map; `focusCall` state → `EnvPanel`; the badge click sets `dockTab='stations'` + `focusCall`)
- Modify: `src/map/MapLibreMap.tsx` (add `preserveDrawingBuffer: true` to the maplibre init)
- Modify: `src/aprs/AprsPositionsMap.tsx` (the export button → `wxSnapshot` canvas compositing)
- Test: extend `src/shell/AppShell` test if one covers the dock; else rely on the map test + wire-walk.

- [ ] **Step 1:** Thread props in `AppShell.tsx`:

```tsx
const [focusCall, setFocusCall] = useState<string | null>(null);
// map (line ~1534):
<AprsPositionsMap
  positions={aprsPositions.positions}
  envStations={envStations.stations}
  operatorGrid={statusData.grid ?? undefined}
  onFocusStation={(call) => { setDockTab('stations'); setFocusCall(call); }}
/>
// EnvPanel (line ~1690):
<EnvPanel stations={envStations.stations} focusCall={focusCall} />
```

- [ ] **Step 2:** In `MapLibreMap.tsx`, add `preserveDrawingBuffer: true` to the `new maplibregl.Map({...})` options so the GL canvas is readable for PNG export. Verify the existing map tests still pass (`pnpm vitest run src/map src/aprs/AprsPositionsMap.test.tsx`).

- [ ] **Step 3:** Add the **export button** (enabled in weather mode) in `AprsPositionsMap`: capture `map.getCanvas().toDataURL('image/png')`, draw it onto a 2D canvas with a header bar rendered from `composeSnapshotHeader({ grid: operatorGrid, utcMs: Date.now(), stationCount: wx.length })`, then trigger a download via an `<a download>` blob. (Guard: `Date.now()` is fine in app code; only workflow scripts ban it.)

- [ ] **Step 4:** Run `pnpm vitest run && pnpm typecheck && pnpm build` → all green.

- [ ] **Step 5: Commit** (`feat(aprs): wire WX overlay into AppShell + PNG snapshot export (ni5b)`).

---

### Task 7: Full gate + Codex review + wire-walk + PR

- [ ] **Step 1:** `pnpm vitest run && pnpm typecheck && pnpm build` — all green.
- [ ] **Step 2:** Push branch; open a draft PR (`[mink-yew-osprey] feat(aprs): on-map weather overlay + category filter (ni5b)`).
- [ ] **Step 3:** Run the independent Codex review on the diff (custom-prompt stdin pattern; audit join correctness, badge honesty, the filter set/reset, the focus seam, canvas export). Triage findings; fix real ones.
- [ ] **Step 4:** Wire-walk (operator supplies flows greenfield): (a) heard WX station shows a temp badge; (b) weather mode hides non-WX pins; (c) clicking a WX badge opens its Station Data card; (d) export produces a PNG with the header.
- [ ] **Step 5:** Mark PR ready + `bd close tuxlink-ni5b` once CI green + wire-walk passes + operator smoke.

---

## Notes for the executor

- **No backend / Rust** — pure frontend; vitest runs locally.
- **maplibre under jsdom has no WebGL/canvas image export** — Tasks 1–4 hold the logic and are unit-tested; Task 5–6's map/export glue is thin and verified by the map double (source/layer/click wiring) + operator grim smoke.
- **Badge honesty is the load-bearing rule** — never assume a sky condition; glyph only from real fields.
- **`preserveDrawingBuffer`** has a small memory cost; if it regresses the existing map, fall back to an on-demand re-render before capture (note in the PR).
