// Authentic APRS symbol sprites for the Tac Chat positions map (tuxlink-90xb).
// Source: hessu/aprs-symbols (CC BY-SA 2.0), vendored at src/assets/aprs-symbols/.
// This module turns an APRS table/code/overlay into a stable MapLibre image id
// and (registration path below) registers the sliced + baked sprite under it.

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
