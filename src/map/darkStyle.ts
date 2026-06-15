/**
 * Baked-dark color transform (tuxlink-ndi4 phase 3, L2/A7).
 *
 * Dark mode is a GL-native inverted style, NOT a runtime CSS filter (the R4 spike
 * measured the CSS filter at ~15fps vs ~45fps for the baked style on the Pi's
 * WebKitGTK path). Each style color is transformed by this module and the result
 * is memoized once per flavor by `basemapStyle.baseLayers` (B3 — the transform is
 * multi-hundred-ms on the Pi, so it must not re-run per style build):
 *
 *   invert(1) → W3C hue-rotate(180°) → brightness(1.33)
 *
 * applied per sRGB channel — the exact `xformHex` the R4 spike proved. The
 * transform walks every `*-color` paint value: literal hex, `rgba()` strings, and
 * color leaves inside data-driven expressions (e.g. a `match` on a feature
 * property), leaving operators / property keys / labels / numbers untouched. This
 * is the belt-and-suspenders coverage A7 requires: after baking, every color in
 * the style is a transformed value (no light color survives).
 *
 * NOTE: sprite icons are raster PNGs and are NOT slot/color-derivable — dark mode
 * swaps to Protomaps' authored `dark` sprite sheet (in basemapStyle), it does not
 * invert the light sprite (A7).
 */

// W3C feColorMatrix matrix for hue-rotate(180deg) (cos=-1, sin=0).
const HUE180 = [
  [-0.574, 1.43, 0.144],
  [0.426, 0.43, 0.144],
  [0.426, 1.43, -0.856],
] as const;

const BRIGHTNESS = 1.33;

function transformChannels(r: number, g: number, b: number): [number, number, number] {
  // r,g,b in [0,1]. invert → hue-rotate → brightness, then to 0..255.
  const ir = 1 - r;
  const ig = 1 - g;
  const ib = 1 - b;
  const nr = HUE180[0][0] * ir + HUE180[0][1] * ig + HUE180[0][2] * ib;
  const ng = HUE180[1][0] * ir + HUE180[1][1] * ig + HUE180[1][2] * ib;
  const nb = HUE180[2][0] * ir + HUE180[2][1] * ig + HUE180[2][2] * ib;
  const clamp = (x: number) => Math.max(0, Math.min(255, Math.round(x * BRIGHTNESS * 255)));
  return [clamp(nr), clamp(ng), clamp(nb)];
}

const hex2 = (x: number) => x.toString(16).padStart(2, '0');

/** Transform a `#rrggbb` color; any other string is returned unchanged. */
export function xformHex(hex: string): string {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return hex;
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const [nr, ng, nb] = transformChannels(r, g, b);
  return `#${hex2(nr)}${hex2(ng)}${hex2(nb)}`;
}

/** Transform an `rgb()` / `rgba()` color, preserving alpha. */
function xformRgba(s: string): string {
  const m = /^rgba?\(([^)]+)\)$/i.exec(s);
  if (!m) return s;
  const parts = m[1].split(',').map((p) => p.trim());
  if (parts.length < 3) return s;
  const r = Number(parts[0]) / 255;
  const g = Number(parts[1]) / 255;
  const b = Number(parts[2]) / 255;
  if (![r, g, b].every((n) => Number.isFinite(n))) return s;
  const [nr, ng, nb] = transformChannels(r, g, b);
  const alpha = parts.length >= 4 ? parts[3] : '1';
  return `rgba(${nr}, ${ng}, ${nb}, ${alpha})`;
}

/** True for strings that are colors we transform (hex6 or rgb/rgba). */
function isColorString(s: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(s) || /^rgba?\([^)]+\)$/i.test(s);
}

/**
 * Transform a paint color VALUE: a literal hex/rgba string, or a data-driven
 * expression (array) whose color-string leaves are transformed in place while
 * operators / property keys / labels / numbers are preserved.
 */
export function transformColorValue(value: unknown): unknown {
  if (typeof value === 'string') {
    if (!isColorString(value)) return value; // label / operator / property key
    return value.startsWith('#') ? xformHex(value) : xformRgba(value);
  }
  if (Array.isArray(value)) {
    return value.map(transformColorValue);
  }
  return value;
}

type Layer = { id: string; type?: string; paint?: Record<string, unknown>; layout?: Record<string, unknown> };

/**
 * Return a deep copy of `layers` with every `*-color` paint value baked to dark.
 * Non-color paint (opacity, width, …) and layout are untouched. The input is not
 * mutated.
 */
export function bakeDarkColors<T extends Layer>(layers: readonly T[]): T[] {
  return layers.map((layer) => {
    if (!layer.paint) return { ...layer };
    const paint: Record<string, unknown> = { ...layer.paint };
    for (const key of Object.keys(paint)) {
      if (key.endsWith('-color')) {
        paint[key] = transformColorValue(paint[key]);
      }
    }
    // The transformed paint keeps the same keys/structure (only color values
    // change), so it is runtime-compatible with the layer's strict paint type.
    return { ...layer, paint } as T;
  });
}
