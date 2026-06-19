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

/** The minimal MapLibre image surface this module drives. */
export interface SpriteMap {
  hasImage(id: string): boolean;
  addImage(id: string, image: unknown, options?: Record<string, unknown>): void;
}

/** Source cell size in the vendored sheets. */
const CELL = 64;

/** Sheet image elements, loaded once. Browser-only; tests stub the 2D context. */
const sheetEls: Record<'p' | 'a' | 'o', HTMLImageElement | null> = { p: null, a: null, o: null };
function sheetFor(which: 'p' | 'a' | 'o'): HTMLImageElement {
  if (!sheetEls[which]) {
    const img = new Image();
    img.src = which === 'p' ? sheetPrimary : which === 'a' ? sheetAlternate : sheetOverlay;
    sheetEls[which] = img;
  }
  return sheetEls[which]!;
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
 * and optionally desaturate. Returns a canvas MapLibre registers via addImage.
 * Canvas-based; pixel correctness is grim-verified (jsdom has no 2D context).
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
  const ctx = canvas.getContext('2d');
  if (!ctx) return canvas; // no 2D context (headless / jsdom) — register an empty image
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
  return canvas;
}

/** A neutral slate dot for unresolved / suppressed symbols — never a tofu box. */
function renderFallbackBitmap(): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = CELL;
  canvas.height = CELL;
  const ctx = canvas.getContext('2d');
  if (!ctx) return canvas; // no 2D context (headless / jsdom) — register an empty image
  ctx.beginPath();
  ctx.arc(CELL / 2, CELL / 2, CELL * 0.28, 0, 2 * Math.PI);
  ctx.fillStyle = '#5b6b7a';
  ctx.fill();
  ctx.lineWidth = 4;
  ctx.strokeStyle = '#cfe0ee';
  ctx.stroke();
  return canvas;
}

/**
 * Idempotently register the colour + greyscale images for a symbol and return the
 * colour id. Brand-logo / unresolved symbols register the neutral fallback pair.
 * Skips the bake entirely when both ids are already present (the lazy fast path).
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
  const make = (grey: boolean): HTMLCanvasElement =>
    id === FALLBACK_ID ? renderFallbackBitmap() : renderSymbolBitmap(table, code, overlay, grey);
  if (!map.hasImage(id)) map.addImage(id, make(false), { pixelRatio: 2 });
  if (!map.hasImage(greyId)) map.addImage(greyId, make(true), { pixelRatio: 2 });
  return id;
}
