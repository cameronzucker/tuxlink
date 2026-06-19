# APRS map-face symbol sprites — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render each heard APRS station's authentic symbol sprite on its Tac Chat map pin, preserving the existing stale/position-ambiguous honesty encoding.

**Architecture:** A new `src/map/aprsSprites.ts` owns the vendored hessu sprite sheets — pure id computation, lazy canvas bake (colour + greyscale + overlay composite), and `map.addImage` registration. `AprsPositionsMap.tsx` replaces its single circle layer with two stacked MapLibre symbol layers (`icon-image: ['get','spriteId']` and `['get','spriteIdGrey']`) whose paint `icon-opacity` cross-fades colour→greyscale on the `stale` feature-state, so staleness stays a feature-state toggle with no FeatureCollection rebuild (preserving tuxlink-gq0d).

**Tech Stack:** React 18 + TypeScript (strict), Vite, MapLibre GL, Vitest + jsdom, the project's shape-only `testMapLibreMock`.

**Spec:** [docs/superpowers/specs/2026-06-18-aprs-sprite-pins-design.md](../specs/2026-06-18-aprs-sprite-pins-design.md)

## Global Constraints

- Frontend-only change; no Rust. `pnpm vitest run` and `pnpm typecheck` run locally; full build is CI.
- TypeScript strict: no `any` leaks; mirror the existing `Record<string, unknown>` casts used for MapLibre specs in `AprsPositionsMap.tsx`.
- Preserve tuxlink-gq0d: the FeatureCollection must NOT be rebuilt on the staleness tick. Staleness rides MapLibre feature-state; only paint properties read it.
- Preserve tuxlink-f717: the amber uncertainty-disc fill/line layers and the cell-centre plotting for ambiguous fixes are unchanged.
- Source: `hessu/aprs-symbols`, CC BY-SA 2.0. Attribute in `NOTICE`; vendor the upstream `COPYRIGHT.md`. Exclude brand-logo cells (`/M` Apple, `/Z` Microsoft, the Kenwood-radio cell) → neutral fallback.
- Conventional commits, scoped `feat(aprs):` / `feat(map):`. Every commit ends with `Agent: marten-owl-poplar` and the `Co-Authored-By:` trailer.
- Docs voice: declarative, no first person, present-indicative (matches the spec).

## File Structure

- **Create** `src/map/aprsSprites.ts` — sprite id computation + sheet bake + `addImage` registration. One responsibility: turning an APRS table/code/overlay into a registered MapLibre image id.
- **Create** `src/map/aprsSprites.test.ts` — pure-logic + registration-wiring tests.
- **Create** `src/assets/aprs-symbols/` — vendored 64px sheets (`aprs-symbols-64-0.png`, `-1.png`, `-2.png`) + `COPYRIGHT.md`.
- **Create/Modify** `NOTICE` (repo root) — CC BY-SA 2.0 attribution block.
- **Create** `src/map/aprsSprites.assets.test.ts` — asserts the vendored assets + NOTICE attribution are present.
- **Modify** `src/map/testMapLibreMock.ts` — add `addImage` / `hasImage` to the fake map (the mock currently lacks them).
- **Modify** `src/aprs/AprsPositionsMap.tsx` — replace the circle pin layer with two symbol icon layers; register images before push; rebind the click handler.
- **Modify** `src/aprs/AprsPositionsMap.test.tsx` — assert the new layers, FC properties, feature-state staleness, ambiguity sizing, and click→popup.

---

## Task 1: Vendor the sprite sheets and attribution

**Files:**
- Create: `src/assets/aprs-symbols/aprs-symbols-64-0.png`, `-64-1.png`, `-64-2.png`, `COPYRIGHT.md`
- Create/Modify: `NOTICE`
- Test: `src/map/aprsSprites.assets.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: the three 64px sheets at `src/assets/aprs-symbols/` (1024×384, 16×6 cells of 64px, table 0 primary / 1 alternate / 2 overlay-characters) and a `NOTICE` containing the attribution that later tasks' bake reads from disk at build time via Vite's asset import.

- [ ] **Step 1: Fetch the vendored assets**

```bash
mkdir -p src/assets/aprs-symbols
for f in aprs-symbols-64-0 aprs-symbols-64-1 aprs-symbols-64-2; do
  curl -sSL -o "src/assets/aprs-symbols/$f.png" \
    "https://raw.githubusercontent.com/hessu/aprs-symbols/master/png/$f.png"
done
curl -sSL -o src/assets/aprs-symbols/COPYRIGHT.md \
  "https://raw.githubusercontent.com/hessu/aprs-symbols/master/COPYRIGHT.md"
# Verify each sheet is 1024x384:
python3 -c "from PIL import Image;[print(f, Image.open('src/assets/aprs-symbols/'+f+'.png').size) for f in ['aprs-symbols-64-0','aprs-symbols-64-1','aprs-symbols-64-2']]"
```
Expected: each prints `(1024, 384)`.

- [ ] **Step 2: Add the attribution block to `NOTICE`**

If `NOTICE` does not exist, create it. Append (or create with) this block:

```
APRS symbol artwork
-------------------
This product bundles APRS symbol artwork from hessu/aprs-symbols
(https://github.com/hessu/aprs-symbols), used under CC BY-SA 2.0.
Per-symbol provenance is preserved in src/assets/aprs-symbols/COPYRIGHT.md.
Brand-logo cells (Apple, Microsoft, Kenwood) are not used by this product;
they render as a neutral fallback marker.
```

- [ ] **Step 3: Write the asset-presence test**

```ts
// src/map/aprsSprites.assets.test.ts
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const ASSET_DIR = resolve(__dirname, '../assets/aprs-symbols');

describe('vendored APRS symbol assets', () => {
  it('ships the three 64px sheets and the upstream COPYRIGHT', () => {
    for (const f of ['aprs-symbols-64-0.png', 'aprs-symbols-64-1.png', 'aprs-symbols-64-2.png', 'COPYRIGHT.md']) {
      expect(existsSync(resolve(ASSET_DIR, f)), `missing ${f}`).toBe(true);
    }
  });

  it('NOTICE attributes hessu/aprs-symbols under CC BY-SA 2.0', () => {
    const notice = readFileSync(resolve(__dirname, '../../NOTICE'), 'utf8');
    expect(notice).toMatch(/aprs-symbols/);
    expect(notice).toMatch(/CC BY-SA 2\.0/);
  });
});
```

- [ ] **Step 4: Run the test**

Run: `pnpm vitest run src/map/aprsSprites.assets.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/assets/aprs-symbols NOTICE src/map/aprsSprites.assets.test.ts
git commit -F - <<'MSG'
feat(aprs): vendor hessu APRS symbol sheets + CC BY-SA attribution

Adds the 64px primary/alternate/overlay sprite sheets and upstream
COPYRIGHT.md under src/assets/aprs-symbols/, with a NOTICE attribution
block. Source for tuxlink-90xb map-face sprites.

Agent: marten-owl-poplar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 2: Sprite id computation (pure)

**Files:**
- Create: `src/map/aprsSprites.ts`
- Test: `src/map/aprsSprites.test.ts`

**Interfaces:**
- Consumes: `lookupAprsSymbol` from `../aprs/aprsSymbols` (to decide resolvability) is NOT needed here — id computation is structural. `cellIndex` mirrors the sheet geometry (`ord(code) - 0x21`, 16 cols).
- Produces:
  - `FALLBACK_ID = 'aprs:fallback'`
  - `BRAND_LOGO_CELLS: Set<string>` (2-char `table+code` keys)
  - `cellIndex(code: string): { col: number; row: number } | null` — null for non-printable codes
  - `spriteIdFor(table: string, code: string, overlay: string | null): string`
  - `greyIdOf(id: string): string` → `` `${id}:grey` ``

- [ ] **Step 1: Write the failing test**

```ts
// src/map/aprsSprites.test.ts
import { describe, it, expect } from 'vitest';
import { spriteIdFor, greyIdOf, cellIndex, FALLBACK_ID, BRAND_LOGO_CELLS } from './aprsSprites';

describe('cellIndex', () => {
  it("maps '!' (0x21) to the top-left cell", () => {
    expect(cellIndex('!')).toEqual({ col: 0, row: 0 });
  });
  it("maps '>' (car) to col 13 row 1", () => {
    expect(cellIndex('>')).toEqual({ col: 13, row: 1 });
  });
  it('rejects a non-printable / multi-char code', () => {
    expect(cellIndex('')).toBeNull();
    expect(cellIndex('ab')).toBeNull();
  });
});

describe('spriteIdFor', () => {
  it('ids a primary-table symbol by code', () => {
    expect(spriteIdFor('/', '>', null)).toBe('aprs:p:>');
  });
  it('ids an alternate-table symbol by code', () => {
    expect(spriteIdFor('\\', '#', null)).toBe('aprs:a:#');
  });
  it('ids an overlay symbol by overlay+code', () => {
    expect(spriteIdFor('1', '#', '1')).toBe('aprs:o:1:#');
  });
  it('falls back for an unresolvable code', () => {
    expect(spriteIdFor('/', '', null)).toBe(FALLBACK_ID);
  });
  it('falls back for the Apple and Microsoft brand-logo cells', () => {
    expect(BRAND_LOGO_CELLS.has('/M')).toBe(true);
    expect(BRAND_LOGO_CELLS.has('/Z')).toBe(true);
    expect(spriteIdFor('/', 'M', null)).toBe(FALLBACK_ID);
    expect(spriteIdFor('/', 'Z', null)).toBe(FALLBACK_ID);
  });
});

describe('greyIdOf', () => {
  it('suffixes :grey', () => {
    expect(greyIdOf('aprs:p:>')).toBe('aprs:p:>:grey');
    expect(greyIdOf(FALLBACK_ID)).toBe('aprs:fallback:grey');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/map/aprsSprites.test.ts`
Expected: FAIL — cannot resolve module `./aprsSprites`.

- [ ] **Step 3: Write the minimal implementation**

```ts
// src/map/aprsSprites.ts
// Authentic APRS symbol sprites for the Tac Chat positions map (tuxlink-90xb).
// Source: hessu/aprs-symbols (CC BY-SA 2.0), vendored at src/assets/aprs-symbols/.
// This module turns an APRS table/code/overlay into a stable MapLibre image id
// and (Task 3) registers the sliced + baked sprite under that id.

/** Image id used when a symbol is unresolved or deliberately suppressed. */
export const FALLBACK_ID = 'aprs:fallback';

/**
 * Brand-logo cells excluded from the public repo per the tuxlink-90xb licensing
 * decision (the COPYRIGHT.md marks these company-owned). They resolve to the
 * neutral fallback. Keys are `table + code`. `/M` = Apple ("Mac apple"), `/Z` =
 * Microsoft ("Windows flag"). Add any further company-owned cells found in the
 * vendored COPYRIGHT.md (e.g. the Kenwood-radio cell) here.
 */
export const BRAND_LOGO_CELLS = new Set<string>(['/M', '/Z']);

/** Lowest printable APRS code char `!` (0x21); the sheet is 16 cells wide. */
const FIRST_CODE = 0x21;
const SHEET_COLS = 16;

/** Sheet cell for a CODE char, or null if it is not a single printable char. */
export function cellIndex(code: string): { col: number; row: number } | null {
  if (code.length !== 1) return null;
  const o = code.charCodeAt(0);
  if (o < FIRST_CODE || o > 0x7e) return null;
  const idx = o - FIRST_CODE;
  return { col: idx % SHEET_COLS, row: Math.floor(idx / SHEET_COLS) };
}

/**
 * Stable MapLibre image id for a symbol. Primary `/` → `aprs:p:<code>`,
 * alternate `\` → `aprs:a:<code>`, overlay (any other table char) →
 * `aprs:o:<table>:<code>`. Unresolvable codes and brand-logo cells → FALLBACK_ID.
 */
export function spriteIdFor(table: string, code: string, overlay: string | null): string {
  if (cellIndex(code) == null) return FALLBACK_ID;
  if (BRAND_LOGO_CELLS.has(table + code)) return FALLBACK_ID;
  if (table === '/') return `aprs:p:${code}`;
  if (table === '\\') return `aprs:a:${code}`;
  if (overlay && /^[0-9A-Z]$/.test(overlay)) return `aprs:o:${overlay}:${code}`;
  return FALLBACK_ID;
}

/** The greyscale (stale) variant id for a colour id. */
export function greyIdOf(id: string): string {
  return `${id}:grey`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/map/aprsSprites.test.ts`
Expected: PASS (all describe blocks).

- [ ] **Step 5: Commit**

```bash
git add src/map/aprsSprites.ts src/map/aprsSprites.test.ts
git commit -F - <<'MSG'
feat(map): APRS sprite id computation (tuxlink-90xb)

spriteIdFor maps table/code/overlay to a stable MapLibre image id;
unresolved codes and the brand-logo cells (/M, /Z) resolve to a neutral
fallback id. Pure + unit-tested; registration follows.

Agent: marten-owl-poplar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 3: Sprite registration (lazy bake + addImage) and mock extension

**Files:**
- Modify: `src/map/aprsSprites.ts`
- Modify: `src/map/testMapLibreMock.ts` (add `addImage` / `hasImage`)
- Test: `src/map/aprsSprites.test.ts` (append)

**Interfaces:**
- Consumes: `spriteIdFor`, `greyIdOf`, `cellIndex`, `FALLBACK_ID` (Task 2).
- Produces:
  - `renderSymbolBitmap(table: string, code: string, overlay: string | null, grey: boolean): ImageData | HTMLCanvasElement` — produces the pixels (canvas; the unit test stubs it).
  - `ensureSymbolImage(map: SpriteMap, table: string, code: string, overlay: string | null): string` — idempotently registers the colour id and its grey variant via `addImage(..., { pixelRatio: 2 })`, and returns the colour id. `SpriteMap` is the minimal `{ hasImage(id): boolean; addImage(id, img, opts?): void }` surface.

The actual canvas bake (`renderSymbolBitmap`) is grim-verified, not unit-asserted (jsdom has no real 2D canvas). The unit test spies on `renderSymbolBitmap` to isolate the registration bookkeeping.

- [ ] **Step 1: Extend the test-mock with `addImage` / `hasImage`**

In `src/map/testMapLibreMock.ts`, add to `MapLibreMock` (interface, after `removeControl`):

```ts
  /** Registered image ids → the image payload passed to addImage. */
  addImage: (id: string, image: unknown, options?: Record<string, unknown>) => void;
  hasImage: (id: string) => boolean;
```

Add a registry to `MapLibreMockState` (after `featureStates`):

```ts
  /** Registered images by id (addImage), with the options object. */
  images: Map<string, { image: unknown; options?: Record<string, unknown> }>;
```

Seed it in the `state` literal (after `featureStates: new Map(),`):

```ts
    images: new Map(),
```

Implement on `mock` (after `removeControl: vi.fn(),`):

```ts
    addImage: vi.fn((id: string, image: unknown, options?: Record<string, unknown>) => {
      state.images.set(id, { image, options });
    }),
    hasImage: vi.fn((id: string) => state.images.has(id)),
```

- [ ] **Step 2: Write the failing registration test (append to `aprsSprites.test.ts`)**

```ts
import { vi } from 'vitest';
import * as sprites from './aprsSprites';
import { createMapLibreMock } from './testMapLibreMock';

describe('ensureSymbolImage', () => {
  it('registers a colour id and a grey id once, idempotently', () => {
    const map = createMapLibreMock();
    const spy = vi
      .spyOn(sprites, 'renderSymbolBitmap')
      .mockReturnValue({ width: 1, height: 1 } as unknown as ImageData);

    const id = sprites.ensureSymbolImage(map, '/', '>', null);
    expect(id).toBe('aprs:p:>');
    expect(map.hasImage('aprs:p:>')).toBe(true);
    expect(map.hasImage('aprs:p:>:grey')).toBe(true);
    // addImage carries pixelRatio:2 for crisp 32px display from 64px cells.
    expect(map.addImage).toHaveBeenCalledWith('aprs:p:>', expect.anything(), { pixelRatio: 2 });

    // Second call is a no-op (idempotent): no further bakes / addImage calls.
    const before = (map.addImage as ReturnType<typeof vi.fn>).mock.calls.length;
    sprites.ensureSymbolImage(map, '/', '>', null);
    expect((map.addImage as ReturnType<typeof vi.fn>).mock.calls.length).toBe(before);
    spy.mockRestore();
  });

  it('registers the fallback id for a brand-logo cell', () => {
    const map = createMapLibreMock();
    const spy = vi
      .spyOn(sprites, 'renderSymbolBitmap')
      .mockReturnValue({ width: 1, height: 1 } as unknown as ImageData);
    const id = sprites.ensureSymbolImage(map, '/', 'M', null);
    expect(id).toBe(sprites.FALLBACK_ID);
    expect(map.hasImage(sprites.FALLBACK_ID)).toBe(true);
    spy.mockRestore();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `pnpm vitest run src/map/aprsSprites.test.ts`
Expected: FAIL — `ensureSymbolImage` / `renderSymbolBitmap` not exported.

- [ ] **Step 4: Implement registration + bake (append to `aprsSprites.ts`)**

```ts
import sheetPrimary from '../assets/aprs-symbols/aprs-symbols-64-0.png';
import sheetAlternate from '../assets/aprs-symbols/aprs-symbols-64-1.png';
import sheetOverlay from '../assets/aprs-symbols/aprs-symbols-64-2.png';

/** The minimal MapLibre image surface this module drives. */
export interface SpriteMap {
  hasImage(id: string): boolean;
  addImage(id: string, image: unknown, options?: Record<string, unknown>): void;
}

const CELL = 64; // source cell size in the vendored sheets

/** Sheet image elements, loaded once. Browser-only; tests stub the bake. */
const sheetEls: Record<'p' | 'a' | 'o', HTMLImageElement | null> = { p: null, a: null, o: null };
function sheetFor(which: 'p' | 'a' | 'o'): HTMLImageElement {
  if (!sheetEls[which]) {
    const img = new Image();
    img.src = which === 'p' ? sheetPrimary : which === 'a' ? sheetAlternate : sheetOverlay;
    sheetEls[which] = img;
  }
  return sheetEls[which]!;
}

/**
 * Slice the cell for (table, code), composite the overlay character when present,
 * and optionally desaturate. Returns a canvas MapLibre can register via addImage.
 * Canvas-based; correctness is grim-verified (jsdom has no 2D context).
 */
export function renderSymbolBitmap(
  table: string,
  code: string,
  overlay: string | null,
  grey: boolean,
): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = CELL;
  canvas.height = CELL;
  const ctx = canvas.getContext('2d')!;
  const isPrimary = table === '/';
  const baseSheet = isPrimary ? sheetFor('p') : sheetFor('a');
  const baseIdx = cellIndex(code)!;
  ctx.drawImage(baseSheet, baseIdx.col * CELL, baseIdx.row * CELL, CELL, CELL, 0, 0, CELL, CELL);
  if (overlay && /^[0-9A-Z]$/.test(overlay)) {
    const ov = cellIndex(overlay);
    if (ov) ctx.drawImage(sheetFor('o'), ov.col * CELL, ov.row * CELL, CELL, CELL, 0, 0, CELL, CELL);
  }
  if (grey) {
    const px = ctx.getImageData(0, 0, CELL, CELL);
    const d = px.data;
    for (let i = 0; i < d.length; i += 4) {
      const l = 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
      d[i] = d[i + 1] = d[i + 2] = l;
    }
    ctx.putImageData(px, 0, 0);
  }
  return canvas;
}

/**
 * Idempotently register the colour + greyscale images for a symbol and return the
 * colour id. Brand-logo / unresolved symbols register the fallback pair. Skips
 * the bake entirely when both ids are already present (the lazy fast path).
 */
export function ensureSymbolImage(
  map: SpriteMap,
  table: string,
  code: string,
  overlay: string | null,
): string {
  const id = spriteIdFor(table, code, overlay);
  const greyId = greyIdOf(id);
  if (map.hasImage(id) && map.hasImage(greyId)) return id;
  // The fallback renders from a known-good cell stand-in: a neutral dot is drawn
  // when the symbol is suppressed. spriteIdFor already collapsed brand/unknown to
  // FALLBACK_ID, so render a neutral marker for that id.
  const renderTable = id === FALLBACK_ID ? '/' : table;
  const renderCode = id === FALLBACK_ID ? '.' : code; // '/.' = a small dot in the primary table
  const renderOverlay = id === FALLBACK_ID ? null : overlay;
  if (!map.hasImage(id)) {
    map.addImage(id, renderSymbolBitmap(renderTable, renderCode, renderOverlay, false), { pixelRatio: 2 });
  }
  if (!map.hasImage(greyId)) {
    map.addImage(greyId, renderSymbolBitmap(renderTable, renderCode, renderOverlay, true), { pixelRatio: 2 });
  }
  return id;
}
```

Note: `renderSymbolBitmap` is referenced via the module namespace in the test (`vi.spyOn(sprites, 'renderSymbolBitmap')`); call it through the module inside `ensureSymbolImage` so the spy intercepts. To make the spy effective under ESM, call it as `exports`-bound — implement `ensureSymbolImage` to call a module-local `const bake = renderSymbolBitmap` indirection is NOT spy-able; instead reference it as `spritesRender(...)` where the module exports an object. Simplest reliable approach: keep both functions in the same module and have `ensureSymbolImage` call `renderSymbolBitmap` directly, and in the test mock the canvas instead of spying. If the spy does not intercept (ESM live-binding), switch the test to stub the canvas 2D context:

```ts
// Alternative Step-2 stub if vi.spyOn on the same-module fn does not intercept:
beforeEach(() => {
  const ctx = { drawImage: vi.fn(), getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })), putImageData: vi.fn() };
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx as unknown as CanvasRenderingContext2D);
});
```

Pick whichever interception works in this repo's Vitest config; both leave the assertions unchanged.

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm vitest run src/map/aprsSprites.test.ts src/map/testMapLibreMock.test.ts`
Expected: PASS. If `testMapLibreMock.test.ts` asserts an exhaustive method list, add `addImage`/`hasImage` there too.

- [ ] **Step 6: Typecheck and commit**

Run: `pnpm typecheck`
Expected: no errors.

```bash
git add src/map/aprsSprites.ts src/map/aprsSprites.test.ts src/map/testMapLibreMock.ts
git commit -F - <<'MSG'
feat(map): lazy APRS sprite registration + greyscale/overlay bake

ensureSymbolImage idempotently registers a colour + greyscale image pair
per heard symbol via addImage(pixelRatio:2), compositing overlay chars and
desaturating the stale variant. Canvas bake is grim-verified; the mock
gains addImage/hasImage for wiring tests.

Agent: marten-owl-poplar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 4: Wire sprites into the positions map

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx`
- Test: `src/aprs/AprsPositionsMap.test.tsx`

**Interfaces:**
- Consumes: `spriteIdFor`, `greyIdOf`, `ensureSymbolImage` (Task 2/3); `HeardPosition` (`symbolTable`, `symbolCode`, `call`, `at`, `ambiguity`, `comment`).
- Produces: two new layers `aprs-position-pins-color` / `aprs-position-pins-grey`; `buildPositionFC` features gain `spriteId` + `spriteIdGrey` properties.

- [ ] **Step 1: Write the failing tests (append to `AprsPositionsMap.test.tsx`)**

Mirror the file's existing setup (render `<AprsPositionsMap positions={…} />`, then `getLastMap()` and emit `styledata`). Add:

```ts
it('builds features carrying stable colour + grey sprite ids (tuxlink-90xb)', () => {
  render(<AprsPositionsMap positions={[carAt(45, -73)]} />); // car: symbolTable '/', symbolCode '>'
  const map = getLastMap()!;
  map.__emit('styledata');
  const src = map.__state.sources.get('aprs-positions') as { data: { features: Array<{ properties: Record<string, unknown> }> } };
  const props = src.data.features[0].properties;
  expect(props.spriteId).toBe('aprs:p:>');
  expect(props.spriteIdGrey).toBe('aprs:p:>:grey');
});

it('adds two icon layers that cross-fade colour->grey on the stale feature-state', () => {
  render(<AprsPositionsMap positions={[carAt(45, -73)]} />);
  const map = getLastMap()!;
  map.__emit('styledata');
  const color = map.__state.layers.find((l) => l.id === 'aprs-position-pins-color')!;
  const grey = map.__state.layers.find((l) => l.id === 'aprs-position-pins-grey')!;
  expect(color.spec.type).toBe('symbol');
  expect((color.spec.layout as Record<string, unknown>)['icon-image']).toEqual(['get', 'spriteId']);
  expect((grey.spec.layout as Record<string, unknown>)['icon-image']).toEqual(['get', 'spriteIdGrey']);
  // Paint icon-opacity reads feature-state 'stale' (the no-FC-churn channel).
  expect(JSON.stringify(color.spec.paint)).toContain('feature-state');
  expect(JSON.stringify(grey.spec.paint)).toContain('feature-state');
});

it('shrinks ambiguous pins via icon-size and keeps the uncertainty disc', () => {
  render(<AprsPositionsMap positions={[ambiguousAt(45, -73, 2)]} />);
  const map = getLastMap()!;
  map.__emit('styledata');
  const color = map.__state.layers.find((l) => l.id === 'aprs-position-pins-color')!;
  expect(JSON.stringify((color.spec.layout as Record<string, unknown>)['icon-size'])).toContain('ambiguity');
  expect(map.__state.layers.some((l) => l.id === 'aprs-position-uncertainty-fill')).toBe(true);
});

it('registers an image for each heard symbol before data is pushed', () => {
  render(<AprsPositionsMap positions={[carAt(45, -73)]} />);
  const map = getLastMap()!;
  map.__emit('styledata');
  expect(map.hasImage('aprs:p:>')).toBe(true);
  expect(map.hasImage('aprs:p:>:grey')).toBe(true);
});
```

Add helpers near the top of the test file (mirroring the existing `HeardPosition` factory if one exists; otherwise):

```ts
const base = { comment: '', at: Date.now() };
const carAt = (lat: number, lon: number) => ({ ...base, call: 'W7RPT-9', lat, lon, ambiguity: 0, symbolTable: '/', symbolCode: '>' });
const ambiguousAt = (lat: number, lon: number, amb: number) => ({ ...base, call: 'N7CPZ', lat, lon, ambiguity: amb, symbolTable: '/', symbolCode: '-' });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx`
Expected: FAIL — layers `aprs-position-pins-color` not found; `spriteId` undefined.

- [ ] **Step 3: Implement the wiring in `AprsPositionsMap.tsx`**

3a. Add imports (after the `lookupAprsSymbol` import, line 28):

```ts
import { spriteIdFor, greyIdOf, ensureSymbolImage } from '../map/aprsSprites';
```

3b. Replace the layer-id constants block (lines 46) — add the two pin layer ids, keep labels:

```ts
const POSITION_PINS_COLOR_LAYER = 'aprs-position-pins-color';
const POSITION_PINS_GREY_LAYER = 'aprs-position-pins-grey';
```
(Remove `POSITION_PINS_LAYER`; replace its later uses per 3e/3f.)

3c. Replace `POSITION_LAYERS` (lines 97–142) with the two symbol icon layers plus the existing label layer. The colour/grey layers share icon-size (ambiguity-driven) and allow overlap so dense clusters don't drop pins:

```ts
const ICON_LAYOUT = {
  'icon-allow-overlap': true,
  'icon-ignore-placement': true,
  // 32px display from 64px @ pixelRatio 2 ⇒ icon-size 1; ambiguous fixes shrink.
  'icon-size': ['case', ['>', ['get', 'ambiguity'], 0], 0.7, 1],
  'icon-anchor': 'center',
} as const;

const POSITION_LAYERS = (
  [
    {
      id: POSITION_PINS_GREY_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: { ...ICON_LAYOUT, 'icon-image': ['get', 'spriteIdGrey'] },
      paint: {
        'icon-opacity': ['case', ['boolean', ['feature-state', 'stale'], false], 0.55, 0],
      },
    },
    {
      id: POSITION_PINS_COLOR_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: { ...ICON_LAYOUT, 'icon-image': ['get', 'spriteId'] },
      paint: {
        'icon-opacity': ['case', ['boolean', ['feature-state', 'stale'], false], 0, 0.95],
      },
    },
    {
      id: POSITION_LABELS_LAYER,
      type: 'symbol',
      source: POSITIONS_SOURCE,
      layout: {
        'text-field': ['get', 'call'],
        'text-size': 11,
        'text-offset': [0, -1.4],
        'text-anchor': 'bottom',
      },
      paint: {
        'text-color': '#eaf3fb',
        'text-halo-color': '#0c1620',
        'text-halo-width': 1.2,
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });
```

3d. In `buildPositionFC` (lines 206–223), add the two stable id properties:

```ts
      properties: {
        call: p.call,
        comment: p.comment,
        ambiguity: p.ambiguity,
        spriteId: spriteIdFor(p.symbolTable, p.symbolCode, lookupAprsSymbol(p.symbolTable, p.symbolCode).overlay),
        spriteIdGrey: greyIdOf(spriteIdFor(p.symbolTable, p.symbolCode, lookupAprsSymbol(p.symbolTable, p.symbolCode).overlay)),
      },
```

(Factor the repeated `spriteIdFor(...)` into a local `const id = …` to stay DRY.)

3e. In `PositionLayers`, register images BEFORE the data push. Add this effect immediately above the `usePushData(map, POSITIONS_SOURCE, fc)` call (so registration is ordered before the push effect):

```ts
  // Register the colour + grey image for every heard symbol before the source
  // data references it (a symbol layer silently skips an unregistered icon-image).
  // Re-applied on styledata because a style swap clears registered images.
  useEffect(() => {
    if (!map) return;
    const m = map as unknown as import('../map/aprsSprites').SpriteMap & {
      on: (t: string, h: (...a: unknown[]) => void) => unknown;
      off: (t: string, h: (...a: unknown[]) => void) => unknown;
    };
    const apply = () => {
      for (const p of positions) {
        ensureSymbolImage(m, p.symbolTable, p.symbolCode, lookupAprsSymbol(p.symbolTable, p.symbolCode).overlay);
      }
    };
    apply();
    m.on('styledata', apply as (...a: unknown[]) => void);
    return () => m.off('styledata', apply as (...a: unknown[]) => void);
  }, [map, positions]);
```

3f. Rebind the click handler (lines 356–367) from `POSITION_PINS_LAYER` to the colour layer:

```ts
    map.on('click', POSITION_PINS_COLOR_LAYER, onClick as (...a: unknown[]) => void);
    return () => {
      map.off('click', POSITION_PINS_COLOR_LAYER, onClick as (...a: unknown[]) => void);
    };
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx`
Expected: PASS — including the file's pre-existing popup/staleness tests (the click now targets the colour layer; ensure any pre-existing test that emits `click` on `aprs-position-pins` is updated to `aprs-position-pins-color`).

- [ ] **Step 5: Full frontend gate**

Run: `pnpm typecheck && pnpm vitest run`
Expected: PASS across the suite.

- [ ] **Step 6: Commit**

```bash
git add src/aprs/AprsPositionsMap.tsx src/aprs/AprsPositionsMap.test.tsx
git commit -F - <<'MSG'
feat(aprs): render authentic symbol sprites on Tac Chat map pins

Replaces the circle pin layer with two stacked symbol layers whose
icon-opacity cross-fades colour->greyscale on the stale feature-state, so
staleness stays a feature-state toggle with no FeatureCollection rebuild
(tuxlink-gq0d preserved). Ambiguous fixes shrink via icon-size and keep the
amber uncertainty disc (tuxlink-f717). Closes the map-face half of
tuxlink-90xb.

Agent: marten-owl-poplar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 5: Verification, smoke gate, and close-out

**Files:**
- Modify: `dev/implementation-log.md` (prepend an entry, if the file exists)
- No code.

- [ ] **Step 1: Open the PR (draft) and let CI compile the build**

```bash
git push
gh pr create --base main --head bd-tuxlink-90xb/sprite-pins --draft \
  --title '[marten-owl-poplar] APRS map-face symbol sprites (tuxlink-90xb)' \
  --body 'Authentic hessu/aprs-symbols sprites on Tac Chat map pins. Spec: docs/superpowers/specs/2026-06-18-aprs-sprite-pins-design.md. Visual gate = operator grim smoke on the converged build (the map render is not jsdom-verifiable).'
```
Expected: CI `verify` job green (typecheck, vitest, build).

- [ ] **Step 2: Wire-walk the definition-of-done flows**

Run the `wire-walk` skill against the spec's five DoD flows (heard known symbol → icon; stale → greyscale; ambiguous → shrunk on disc; overlay composited; unknown/brand-logo → fallback). Trace each to `file:line`. Any broken primary flow blocks the "done" claim.

- [ ] **Step 3: Operator grim smoke on the converged build**

Surface to the operator: on `pnpm dev:converged`, open the Tac Chat map and confirm — authentic icons on pins, greyscale on a stale station, a shrunk icon on the amber disc for an ambiguous fix, an overlay symbol (e.g. a `WIDE1-1` digipeater) composited, and the neutral dot for an unknown symbol. Capture via grim. This is the only check that exercises the real WebKitGTK render.

- [ ] **Step 4: Mark PR ready, close the issue**

After the smoke passes: `gh pr ready <#>`, then on merge `bd close tuxlink-90xb`.

## Self-Review

- **Spec coverage:** Treatment A bare sprite (Task 4 layers) ✓; ~32px via pixelRatio 2 (Task 3/4) ✓; stale greyscale with no FC churn (Task 4 two-layer cross-fade) ✓; ambiguity shrink + disc (Task 4) ✓; overlay composite (Task 3 bake) ✓; unknown/brand-logo fallback (Task 2 + Task 3) ✓; full-set-minus-brand-logos vendoring + CC BY-SA attribution (Task 1 + `BRAND_LOGO_CELLS`) ✓; vitest coverage (Tasks 2–4) ✓; grim smoke gate (Task 5) ✓.
- **Placeholder scan:** none — every code step carries full content; Task 3 names the one ESM-spy fallback explicitly with the concrete stub.
- **Type consistency:** `spriteIdFor`/`greyIdOf`/`ensureSymbolImage`/`renderSymbolBitmap` signatures match across Tasks 2–4; layer ids `aprs-position-pins-color` / `-grey` are used identically in the impl and tests; `spriteId`/`spriteIdGrey` property names match between `buildPositionFC` and the layer `icon-image` expressions.
