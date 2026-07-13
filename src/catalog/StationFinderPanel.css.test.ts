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
