import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  spriteIdFor,
  greyIdOf,
  cellIndex,
  ensureSymbolImage,
  FALLBACK_ID,
  BRAND_LOGO_CELLS,
} from './aprsSprites';
import { createMapLibreMock } from './testMapLibreMock';

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

describe('ensureSymbolImage', () => {
  // jsdom has no 2D canvas context; stub it so the bake runs without real canvas.
  // Pixel correctness is grim-verified, not asserted here.
  beforeEach(() => {
    const ctx = {
      drawImage: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(64 * 64 * 4) })),
      putImageData: vi.fn(),
      beginPath: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
      stroke: vi.fn(),
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 0,
    };
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(
      ctx as unknown as CanvasRenderingContext2D,
    );
  });
  afterEach(() => vi.restoreAllMocks());

  it('registers a colour id and a grey id once, idempotently', () => {
    const map = createMapLibreMock();
    const id = ensureSymbolImage(map, '/', '>', null);
    expect(id).toBe('aprs:p:>');
    expect(map.hasImage('aprs:p:>')).toBe(true);
    expect(map.hasImage('aprs:p:>:grey')).toBe(true);
    // addImage carries pixelRatio:2 for crisp 32px display from 64px cells.
    expect(map.addImage).toHaveBeenCalledWith('aprs:p:>', expect.anything(), { pixelRatio: 2 });

    const before = (map.addImage as ReturnType<typeof vi.fn>).mock.calls.length;
    ensureSymbolImage(map, '/', '>', null); // second call is a no-op
    expect((map.addImage as ReturnType<typeof vi.fn>).mock.calls.length).toBe(before);
  });

  it('registers the neutral fallback pair for a brand-logo cell', () => {
    const map = createMapLibreMock();
    const id = ensureSymbolImage(map, '/', 'M', null);
    expect(id).toBe(FALLBACK_ID);
    expect(map.hasImage(FALLBACK_ID)).toBe(true);
    expect(map.hasImage(`${FALLBACK_ID}:grey`)).toBe(true);
  });
});
