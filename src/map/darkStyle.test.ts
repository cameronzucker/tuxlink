/**
 * Tests for the baked-dark color transform (tuxlink-ndi4 phase 3, L2/A7).
 *
 * Dark mode is a GL-native inverted style: each style color is transformed at
 * build time (invert → W3C hue-rotate(180°) → brightness(1.33)), matching the R4
 * spike's xformHex. The transform covers literal hex, rgba() leaves, and color
 * leaves inside data-driven expressions — while leaving operators/labels/numbers
 * untouched (A7: "every *-color is transformed-hex or an expression with
 * transformed color leaves").
 */
import { describe, it, expect } from 'vitest';
import { xformHex, transformColorValue, bakeDarkColors } from './darkStyle';

describe('xformHex', () => {
  it('transforms a mid-gray to dark-gray (#cccccc → #444444)', () => {
    // 0.8 → invert 0.2 → hue-rotate keeps it gray → *1.33*255 ≈ 68 = 0x44.
    expect(xformHex('#cccccc')).toBe('#444444');
  });

  it('changes a saturated color and stays a 6-digit hex', () => {
    const out = xformHex('#80deea');
    expect(out).toMatch(/^#[0-9a-f]{6}$/);
    expect(out).not.toBe('#80deea');
  });

  it('passes non-hex strings through unchanged', () => {
    expect(xformHex('grassland')).toBe('grassland');
    expect(xformHex('#abc')).toBe('#abc'); // 3-digit not handled
  });
});

describe('transformColorValue', () => {
  it('transforms an rgba() string and preserves alpha', () => {
    const out = transformColorValue('rgba(210, 239, 207, 1)') as string;
    expect(out).toMatch(/^rgba\(\d+, \d+, \d+, 1\)$/);
    expect(out).not.toBe('rgba(210, 239, 207, 1)');
  });

  it('recurses into expressions, transforming color leaves but not labels/operators', () => {
    const expr = ['match', ['get', 'kind'], 'grassland', '#cccccc', 'barren', 'rgba(255, 243, 215, 1)', '#80deea'];
    const out = transformColorValue(expr) as unknown[];
    expect(out[0]).toBe('match');
    expect(out[1]).toEqual(['get', 'kind']);
    expect(out[2]).toBe('grassland'); // label untouched
    expect(out[3]).toBe('#444444'); // color transformed
    expect(out[4]).toBe('barren'); // label untouched
    expect(out[5]).toMatch(/^rgba\(/); // rgba transformed
    expect(out[5]).not.toBe('rgba(255, 243, 215, 1)');
  });
});

describe('bakeDarkColors', () => {
  it('transforms every *-color paint value (string + expression) in place of a copy', () => {
    const layers = [
      { id: 'bg', type: 'background', paint: { 'background-color': '#cccccc' } },
      {
        id: 'land',
        type: 'fill',
        paint: {
          'fill-color': ['match', ['get', 'kind'], 'a', '#80deea', '#cccccc'],
          'fill-opacity': 0.5, // non-color paint left alone
        },
      },
      { id: 'lbl', type: 'symbol', layout: { 'text-field': ['get', 'name'] }, paint: { 'text-halo-color': '#cccccc' } },
    ];
    const dark = bakeDarkColors(layers);
    expect((dark[0].paint as Record<string, unknown>)['background-color']).toBe('#444444');
    const land = dark[1].paint as Record<string, unknown>;
    expect((land['fill-color'] as unknown[])[3]).toBe('#80deea' === '#80deea' ? xformHex('#80deea') : '');
    expect(land['fill-opacity']).toBe(0.5); // untouched
    expect((dark[2].paint as Record<string, unknown>)['text-halo-color']).toBe('#444444');
    // Original not mutated.
    expect((layers[0].paint as Record<string, unknown>)['background-color']).toBe('#cccccc');
  });
});
