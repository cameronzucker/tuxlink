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
      // Faithful ImageData shape (width/height + matching-length data): the bake
      // returns ctx.getImageData(...) and hands it to addImage, which validates
      // data.length === width*height*4 (tuxlink-r8sm regression below).
      getImageData: vi.fn(() => ({
        width: 64,
        height: 64,
        data: new Uint8ClampedArray(64 * 64 * 4),
      })),
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

  // Regression: tuxlink-r8sm. PR #819 passed a raw <canvas> to addImage; MapLibre
  // 5.x only accepts HTMLImageElement | ImageData | ImageBitmap | {width,height,
  // data}. A canvas has .width/.height but no .data, so MapLibre built a
  // zero-length buffer and threw "mismatched image size. expected: 0 but got:
  // 16384", which the ErrorBoundary caught — the map "crash" on open. This mock
  // reproduces MapLibre's exact contract so the bake must hand it valid ImageData.
  it('hands addImage a valid ImageData MapLibre accepts (no raw canvas)', () => {
    const map = createMapLibreMock();
    const enforceMapLibreContract = (id: string, image: unknown): void => {
      const img = image as { width?: number; height?: number; data?: { length: number } };
      if (img == null || img.width === undefined || img.height === undefined) {
        throw new Error('Invalid arguments to map.addImage()');
      }
      const len = img.data ? img.data.length : 0;
      if (len !== img.width * img.height * 4) {
        throw new RangeError(
          `mismatched image size. expected: ${len} but got: ${img.width * img.height * 4}`,
        );
      }
      map.__state.images.set(id, { image, options: undefined });
    };
    (map.addImage as ReturnType<typeof vi.fn>).mockImplementation(enforceMapLibreContract);

    expect(() => ensureSymbolImage(map, '/', '>', null)).not.toThrow();
    expect(map.hasImage('aprs:p:>')).toBe(true);
    expect(map.hasImage('aprs:p:>:grey')).toBe(true);
  });

  // tuxlink-r8sm self-heal: the first paint bakes before the sheets decode, so the
  // map re-bakes with force once they're ready. force must re-register an
  // already-present image (via updateImage when available), not no-op.
  it('force re-bakes an already-registered symbol via updateImage', () => {
    const map = createMapLibreMock();
    const updateImage = vi.fn();
    (map as unknown as { updateImage: typeof updateImage }).updateImage = updateImage;

    ensureSymbolImage(map, '/', '>', null); // initial bake (addImage x2)
    expect(updateImage).not.toHaveBeenCalled();

    ensureSymbolImage(map, '/', '>', null, true); // forced re-bake
    expect(updateImage).toHaveBeenCalledWith('aprs:p:>', expect.anything());
    expect(updateImage).toHaveBeenCalledWith('aprs:p:>:grey', expect.anything());
  });
});
