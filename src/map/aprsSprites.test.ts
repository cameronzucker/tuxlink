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
    expect(spriteIdFor('/', '', null)).toBe(FALLBACK_ID);
  });
  it('falls back for the Apple, Microsoft, and Kenwood brand-logo cells', () => {
    expect(BRAND_LOGO_CELLS.has('/M')).toBe(true);
    expect(BRAND_LOGO_CELLS.has('/Z')).toBe(true);
    expect(BRAND_LOGO_CELLS.has('\\K')).toBe(true);
    expect(spriteIdFor('/', 'M', null)).toBe(FALLBACK_ID);
    expect(spriteIdFor('/', 'Z', null)).toBe(FALLBACK_ID);
    expect(spriteIdFor('\\', 'K', null)).toBe(FALLBACK_ID);
  });
});

describe('greyIdOf', () => {
  it('suffixes :grey', () => {
    expect(greyIdOf('aprs:p:>')).toBe('aprs:p:>:grey');
    expect(greyIdOf(FALLBACK_ID)).toBe('aprs:fallback:grey');
  });
});
