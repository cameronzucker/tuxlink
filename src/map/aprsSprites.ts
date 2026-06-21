// Authentic APRS symbol sprites for the Tac Chat positions map (tuxlink-90xb).
// Source: hessu/aprs-symbols (CC BY-SA 2.0), vendored at src/assets/aprs-symbols/.
// This module turns an APRS table/code/overlay into a stable MapLibre image id
// and (registration path below) registers the sliced + baked sprite under it.

import sheetPrimary from '../assets/aprs-symbols/aprs-symbols-64-0.png';
import sheetAlternate from '../assets/aprs-symbols/aprs-symbols-64-1.png';
import sheetOverlay from '../assets/aprs-symbols/aprs-symbols-64-2.png';

/** Image id used when a symbol is unresolved or deliberately suppressed. */
export const FALLBACK_ID = 'aprs:fallback';

/**
 * Brand-logo cells excluded from the public repo per the tuxlink-90xb licensing
 * decision (the vendored COPYRIGHT.md marks these company-owned). They resolve to
 * the neutral fallback. Keys are `table + code`:
 *   `/M` = Apple ("Mac apple"), `/Z` = Microsoft ("Windows flag"),
 *   `\K` = Kenwood ("Kenwood radio" — confirmed via ALTERNATE_SYMBOLS ordering,
 *          which matches the COPYRIGHT.md entry order around the Kenwood cell).
 */
export const BRAND_LOGO_CELLS = new Set<string>(['/M', '/Z', '\\K']);

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
 * Stable MapLibre image id for a symbol. Primary `/` -> `aprs:p:<code>`,
 * alternate `\` -> `aprs:a:<code>`, overlay (any other table char) ->
 * `aprs:o:<overlay>:<code>`. Unresolvable codes and brand-logo cells -> FALLBACK_ID.
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

/** Source cell size in the vendored sheets. */
const CELL = 64;

type SheetKey = 'p' | 'a' | 'o';
const SHEET_SRC: Record<SheetKey, string> = { p: sheetPrimary, a: sheetAlternate, o: sheetOverlay };

/** Sheet image elements, loaded once. Browser-only; tests stub the 2D context. */
const sheetEls: Record<SheetKey, HTMLImageElement | null> = { p: null, a: null, o: null };
/** Callbacks waiting for all sheets to finish decoding (see `whenSheetsReady`). */
const readyWaiters = new Set<() => void>();

function flushIfReady(): void {
  if (!sheetsReady()) return;
  const pending = [...readyWaiters];
  readyWaiters.clear();
  for (const cb of pending) cb();
}

function sheetFor(which: SheetKey): HTMLImageElement {
  let img = sheetEls[which];
  if (!img) {
    img = new Image();
    // Re-bake transparent first-paint sprites once a sheet decodes (r8sm): the
    // first bake runs synchronously on map mount, before the PNGs are decoded.
    img.onload = flushIfReady;
    img.src = SHEET_SRC[which];
    sheetEls[which] = img;
  }
  return img;
}

/** True once all three sheets have decoded (browser). False in jsdom, where
 *  images never load — bakes there draw nothing and pixel correctness is
 *  grim-verified, not unit-asserted. */
export function sheetsReady(): boolean {
  return (['p', 'a', 'o'] as const).every((k) => {
    const s = sheetEls[k];
    return !!s && s.complete && s.naturalWidth > 0;
  });
}

/**
 * Run `cb` once all three sprite sheets have decoded — immediately if they
 * already have. Returns an unsubscribe. The map uses this to re-bake (with
 * `force`) the sprites it registered transparent on the first synchronous paint,
 * before the sheets had decoded (tuxlink-r8sm). Touches all three sheets so their
 * decode starts even if only one table's symbols have been heard so far.
 */
export function whenSheetsReady(cb: () => void): () => void {
  sheetFor('p');
  sheetFor('a');
  sheetFor('o');
  if (sheetsReady()) {
    cb();
    return () => {};
  }
  readyWaiters.add(cb);
  return () => {
    readyWaiters.delete(cb);
  };
}

/**
 * A blank (transparent) image MapLibre accepts, for the no-2D-context path
 * (headless / jsdom). A plain `{width,height,data}` object — NOT `new
 * ImageData()`, which is undefined in jsdom — with a correctly-sized buffer so
 * `addImage` never sees the zero-length mismatch that crashed the map (r8sm).
 */
function blankImage(): ImageData {
  return {
    width: CELL,
    height: CELL,
    data: new Uint8ClampedArray(CELL * CELL * 4),
  } as ImageData;
}

/** Desaturate a 64×64 canvas in place (luma) for the stale variant. */
function desaturate(ctx: CanvasRenderingContext2D): void {
  const px = ctx.getImageData(0, 0, CELL, CELL);
  const d = px.data;
  for (let i = 0; i < d.length; i += 4) {
    const l = 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
    d[i] = d[i + 1] = d[i + 2] = l;
  }
  ctx.putImageData(px, 0, 0);
}

/**
 * Slice the cell for (table, code), composite the overlay character when present,
 * and optionally desaturate. Returns the baked pixels as `ImageData`.
 *
 * MUST return `ImageData`, not the `<canvas>` — MapLibre's `addImage` accepts only
 * `HTMLImageElement | ImageData | ImageBitmap | {width,height,data}`; a raw canvas
 * has no `.data`, so MapLibre reads a zero-length buffer and throws
 * `RangeError: mismatched image size. expected: 0 but got: <w*h*4>`, crashing the
 * map render (tuxlink-r8sm / PR #819). Pixel correctness is grim-verified
 * (jsdom has no real 2D context).
 */
export function renderSymbolBitmap(
  table: string,
  code: string,
  overlay: string | null,
  grey: boolean,
): ImageData {
  const canvas = document.createElement('canvas');
  canvas.width = CELL;
  canvas.height = CELL;
  const ctx = canvas.getContext('2d');
  if (!ctx) return blankImage(); // no 2D context (headless / jsdom)
  // Primary `/` draws the primary sheet; everything else (alternate `\` and any
  // overlay table char) draws the alternate base, then the overlay char on top.
  const baseSheet = table === '/' ? sheetFor('p') : sheetFor('a');
  const base = cellIndex(code)!;
  ctx.drawImage(baseSheet, base.col * CELL, base.row * CELL, CELL, CELL, 0, 0, CELL, CELL);
  if (overlay && /^[0-9A-Z]$/.test(overlay)) {
    const ov = cellIndex(overlay);
    if (ov) ctx.drawImage(sheetFor('o'), ov.col * CELL, ov.row * CELL, CELL, CELL, 0, 0, CELL, CELL);
  }
  if (grey) desaturate(ctx);
  return ctx.getImageData(0, 0, CELL, CELL);
}

/** A neutral slate dot for unresolved / suppressed symbols — never a tofu box.
 *  Returns `ImageData` for the same MapLibre-contract reason as
 *  [`renderSymbolBitmap`] (tuxlink-r8sm). */
function renderFallbackBitmap(): ImageData {
  const canvas = document.createElement('canvas');
  canvas.width = CELL;
  canvas.height = CELL;
  const ctx = canvas.getContext('2d');
  if (!ctx) return blankImage(); // no 2D context (headless / jsdom)
  ctx.beginPath();
  ctx.arc(CELL / 2, CELL / 2, CELL * 0.28, 0, 2 * Math.PI);
  ctx.fillStyle = '#5b6b7a';
  ctx.fill();
  ctx.lineWidth = 4;
  ctx.strokeStyle = '#cfe0ee';
  ctx.stroke();
  return ctx.getImageData(0, 0, CELL, CELL);
}

/**
 * Bake a symbol to a PNG `data:` URL for a Leaflet `divIcon` `<img src>`
 * (tuxlink-6kdw). The Leaflet substrate has no MapLibre `addImage` sprite atlas;
 * each pin is a DOM `divIcon` whose `<img>` carries this URL. `grey=true` bakes
 * the desaturated stale variant. Routes through the SAME fallback/brand-logo
 * branch as [`ensureSymbolImage`] so unresolved/brand cells get the neutral dot,
 * not a broken image.
 *
 * Returns `''` in jsdom (no 2D context / `toDataURL` unimplemented) — callers must
 * NOT rely on the URL for identity in unit tests (assert via [`spriteIdFor`]
 * instead); real pixels are grim-verified. `''` is a harmless empty `<img src>`.
 */
export function spriteDataUrl(table: string, code: string, overlay: string | null, grey: boolean): string {
  const id = spriteIdFor(table, code, overlay);
  const img = id === FALLBACK_ID ? renderFallbackBitmap() : renderSymbolBitmap(table, code, overlay, grey);
  const canvas = document.createElement('canvas');
  canvas.width = CELL;
  canvas.height = CELL;
  const ctx = canvas.getContext('2d');
  if (!ctx) return ''; // headless / jsdom — no real raster
  try {
    ctx.putImageData(img, 0, 0);
    return canvas.toDataURL('image/png');
  } catch {
    return '';
  }
}

