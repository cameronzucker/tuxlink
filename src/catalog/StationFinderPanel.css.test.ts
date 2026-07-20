// QA round-3 finding 5 regression guard. The map overlays
// (`.station-finder__layers` + `.station-finder__reachkey`) vanished twice:
// first shipped with NO z-index (under Leaflet's panes, z 200-700), then the
// z-index:1100 fix was inserted ABOVE a leftover `z-index: 7` in the same
// rule — last declaration wins, so the fix was dead on arrival and survived a
// grep-for-presence verification. This test pins the failure class at the
// source: each overlay rule carries exactly ONE z-index, and it clears
// Leaflet's control container (1000).
//
// Raw-import per pitfall TEST-1 (no node:fs in tests) — the AppShell.test.tsx
// `import.meta.glob` + `?raw` CSS contract-test pattern.
import { describe, expect, it } from 'vitest';

const CSS_MODULES = import.meta.glob('./StationFinderPanel.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const css = CSS_MODULES['./StationFinderPanel.css'];

/** The full declaration block for the FIRST `selector {` rule in the sheet,
 *  with comments stripped (the finding-5 explanation comment itself names
 *  `z-index: 7` — only real declarations may count). */
function ruleBlock(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start, `rule ${selector} exists`).toBeGreaterThanOrEqual(0);
  const end = css.indexOf('}', start);
  return css.slice(start, end).replace(/\/\*[\s\S]*?\*\//g, '');
}

describe.each(['.station-finder__layers', '.station-finder__reachkey'])(
  'map overlay rule %s',
  (selector) => {
    it('declares exactly one z-index, above Leaflet panes AND its control container', () => {
      const block = ruleBlock(selector);
      const decls = [...block.matchAll(/z-index:\s*(-?\d+)/g)];
      expect(decls, 'exactly one z-index declaration (a duplicate silently wins)').toHaveLength(1);
      expect(Number(decls[0][1])).toBeGreaterThan(1000);
    });
  },
);

// Task 11 (tuxlink-6i0ie) containment guard. EVIDENCE (dev/scratch/si-containment,
// harness ?view=finder&ring=0|240, 1366x1200 @8000ms settle): `.station-finder`
// grew from a 1028px content-driven height (ring=0) to 1104px (ring=240), EXACTLY
// 92vh of the 1200px window, i.e. the panel WAS growing content-driven and got
// stopped only by the max-height clamp, which the operator perceives as "the
// window resizing." A bare `max-height` with no `height` lets the panel's OWN
// height float with content up to the clamp; a fixed `height` removes that
// degree of freedom entirely: the panel is the same size regardless of how
// much the strip below wants to grow.
describe('.station-finder fixed-height containment (tuxlink-6i0ie)', () => {
  it('declares exactly one height, and no bare max-height-only sizing', () => {
    const block = ruleBlock('.station-finder');
    const heightDecls = [...block.matchAll(/(?<!max-|min-)height:\s*[^;]+;/g)];
    expect(heightDecls, 'exactly one `height` declaration').toHaveLength(1);
    // tuxlink-qldzn amendment: the containment invariant is that height is
    // CONTENT-independent, not viewport-independent. The former min(760px, 92vh)
    // pinned the panel to a 760px island on large displays (R2 2160x1440
    // operator report); viewport units keep it deterministic per display while
    // letting the feature use the screen it is given.
    expect(heightDecls[0][0]).toContain('92vh');
    expect(heightDecls[0][0], 'no px cap: the panel scales with the viewport').not.toMatch(/\d+px/);
    expect(block, 'no bare max-height-only sizing left in the rule').not.toMatch(/(?<!min-)max-height:/);
  });

  it('width scales with the viewport (tuxlink-qldzn: no px island cap)', () => {
    const block = ruleBlock('.station-finder');
    const widthDecls = [...block.matchAll(/(?<!max-|min-)width:\s*[^;]+;/g)];
    expect(widthDecls, 'exactly one `width` declaration').toHaveLength(1);
    expect(widthDecls[0][0]).toContain('96vw');
    expect(widthDecls[0][0], 'no px cap on width').not.toMatch(/\d+px/);
  });

  it('rail is width-capped so surplus panel width flows to the map (tuxlink-qldzn)', () => {
    const block = ruleBlock('.station-finder__rail');
    expect(block, 'rail declares a max-width').toMatch(/max-width:\s*\d+px;/);
  });
});

// Task 11 / tuxlink-1w0d0: the WWV no-copy manual-entry row previously forced
// `flex-basis: 100%`, a full-width second line even when the row's actual
// content (play-clip button + 3 small SFI/A/K fields) is far narrower, which
// read as a blank band under the actions row. It should wrap WITHIN the
// actions row like its sibling `.station-finder__offair-note`/`__offair`
// classes, bounded by a max-width so it never re-claims the full row either.
describe('.station-finder__offair-nocopy contained width (tuxlink-1w0d0)', () => {
  it('drops the full-width flex-basis and declares a max-width', () => {
    const block = ruleBlock('.station-finder__offair-nocopy');
    expect(block, 'no more flex-basis: 100% (the old full-width forcing rule)').not.toMatch(/flex-basis:\s*100%/);
    expect(block).toMatch(/max-width:\s*[\d.]+(px|ch|rem)/);
  });
});
