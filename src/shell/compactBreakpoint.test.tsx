import { describe, it, expect } from 'vitest';
import { COMPACT_MEDIA_QUERY } from './useViewport';

// tuxlink-mt73: the FZ-M1 compact ("touch") treatment must gate on an actual
// touch-capable device, not on viewport width alone. Before this fix, the
// compact rules fired purely on `@media (max-width: 1365px)`, so the separate
// Compose webview (a fixed 1100px-wide window) tripped them on a normal
// desktop. The fix adds `and (any-pointer: coarse)` to EVERY compact breakpoint
// (CSS) and to the shared JS COMPACT_MEDIA_QUERY, so matchMedia and the CSS
// stay byte-identical (the tuxlink-h7q7 "CSS and JS can never disagree"
// invariant) and a mouse-only desktop never enters compact, regardless of
// window width.

// Raw-import every stylesheet under src/. import.meta.glob needs an object
// literal (Vite statically analyzes it) — see TEST-1 pitfall.
const ALL_CSS = import.meta.glob('/src/**/*.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

const BARE_RULE = /@media \(max-width: 1365px\)\s*\{/;
const GATED_RULE = '@media (max-width: 1365px) and (any-pointer: coarse) {';

describe('FZ-M1 compact breakpoint is touch-gated (tuxlink-mt73)', () => {
  it('the shared JS media query gates on a coarse (touch) pointer', () => {
    expect(COMPACT_MEDIA_QUERY).toBe('(max-width: 1365px) and (any-pointer: coarse)');
  });

  it('no stylesheet leaves a bare width-only compact @media rule', () => {
    for (const [path, css] of Object.entries(ALL_CSS)) {
      expect(
        BARE_RULE.test(css),
        `${path} has a width-only compact @media rule — it must gate on ` +
          `'and (any-pointer: coarse)' so a narrow desktop window (e.g. the ` +
          `1100px Compose webview) does not enter FZ-M1 touch mode`,
      ).toBe(false);
    }
  });

  it('the compact stylesheets carry the touch-gated breakpoint', () => {
    const gated = Object.values(ALL_CSS).filter((css) => css.includes(GATED_RULE));
    // Sanity floor: the FZ-M1 compact surfaces (>=19 files) must be present and
    // gated, so a future bare reintroduction is caught by the rule above.
    expect(gated.length).toBeGreaterThanOrEqual(19);
  });
});
