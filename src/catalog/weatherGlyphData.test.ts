import { describe, it, expect } from 'vitest';
import { resolveGlyph, conditionTextClass } from './weatherGlyphData';

describe('resolveGlyph (tuxlink-n6tp)', () => {
  it('decodes core SFT codes to plain-English labels', () => {
    expect(resolveGlyph('Sunny')?.label).toBe('Sunny');
    expect(resolveGlyph('Vryhot')?.label).toBe('Very hot');
    expect(resolveGlyph('Mosunny')?.label).toBe('Mostly sunny');
    expect(resolveGlyph('Ptcldy')?.label).toBe('Partly cloudy');
    expect(resolveGlyph('Mocldy')?.label).toBe('Mostly cloudy');
  });

  it('is case- and whitespace/punctuation-insensitive', () => {
    expect(resolveGlyph('  ptcldy ')?.kind).toBe('ptcldy');
    expect(resolveGlyph('VRYHOT')?.label).toBe('Very hot');
    expect(resolveGlyph('Mostly Sunny')?.kind).toBe('mosunny');
  });

  it('aliases abbreviated / spelled-out variants to one kind', () => {
    expect(resolveGlyph('Tstms')?.kind).toBe('tstms');
    expect(resolveGlyph('Rnshwrs')?.kind).toBe('showers');
    expect(resolveGlyph('Blgdust')?.kind).toBe('dust');
    expect(resolveGlyph('Flurries')?.kind).toBe('snow');
  });

  it('collapses Sunny / Hot / Vryhot to one sun shape, differing only by accent', () => {
    expect(resolveGlyph('Sunny')?.kind).toBe('sunny');
    expect(resolveGlyph('Hot')?.kind).toBe('sunny');
    expect(resolveGlyph('Vryhot')?.kind).toBe('sunny');
    expect(resolveGlyph('Sunny')?.accent).toBe('sun');
    expect(resolveGlyph('Hot')?.accent).toBe('hot');
    expect(resolveGlyph('Vryhot')?.accent).toBe('danger');
  });

  it('returns null for an unknown code so the caller can fall back to text', () => {
    expect(resolveGlyph('Wxyz')).toBeNull();
    expect(resolveGlyph('')).toBeNull();
    expect(resolveGlyph('MM')).toBeNull();
  });
});

describe('conditionTextClass (fallback heat-accent, parity with legacy condClass)', () => {
  it('keeps the legacy heat classes for the unmapped-text path', () => {
    expect(conditionTextClass('Vryhot')).toBe('cond vryhot');
    expect(conditionTextClass('Hot')).toBe('cond hot');
    expect(conditionTextClass('Sunny')).toBe('cond');
  });
});
