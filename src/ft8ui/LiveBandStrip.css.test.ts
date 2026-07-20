// Task 11 (tuxlink-6i0ie) containment guard, same raw-CSS pattern as
// StationFinderPanel.css.test.ts (`?raw` import + first-rule-block scan; TEST-1
// forbids node:fs in tests, so this reads the sheet as a vite-transformed
// string, not off disk).
//
// EVIDENCE (dev/scratch/si-containment, harness ?view=finder&ring=0|240,
// 1366x1200 @8000ms settle):
//   .si-strip__body   198px -> 323px  (+125px)
//   .si-wf (waterfall) 197px -> 322px (+125px) -- the CANVAS inside stays a
//     fixed 168px the whole time, so that +125px is DEAD SPACE opening up
//     below the waterfall, exactly the operator's "leaving dead negative
//     space below the waterfall" bug report. `.si-wf` and `.si-feed` are
//     flex-row siblings under default align-items:stretch, so `.si-feed`'s
//     unbounded decode-feed table dragging `.si-strip__body` taller drags
//     `.si-wf` along with it even though the canvas itself never changes.
//   .si-feed           197px -> 322px (+125px) -- the actual driver: DecodeFeed
//     (Decode Feed.css) already declares `overflow: auto; min-height: 0`, but
//     nothing above it in the chain has a BOUNDED height for that overflow to
//     ever engage against, so the table just grows the box instead of
//     scrolling inside it.
// A fixed height on the live body, with the waterfall and feed columns as
// `min-height: 0` flex children, gives DecodeFeed's own overflow:auto
// somewhere bounded to scroll within -- the panel stops growing regardless of
// how many decodes are in the ring.
//
// Fix round 1 (reviewer finding): the fixed height rides on the
// `.si-strip__body--live` MODIFIER, applied by LiveBandStrip.tsx only on the
// waterfall+feed branch (showLiveBody). The bare `.si-strip__body` also wraps
// NonLiveBody (off / transitional / needs-setup / device-lost), whose
// `.si-strip__notice--setup` child is deliberately tuned to grow to
// min(52vh, 480px); an unconditional 200px would squeeze the Ft8StripSetup
// onboarding form into a scroll box, re-creating the 2026-07-12
// CTA-off-the-bottom bug.
import { describe, expect, it } from 'vitest';

const CSS_MODULES = import.meta.glob('./LiveBandStrip.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const css = CSS_MODULES['./LiveBandStrip.css'];

/** The full declaration block for the FIRST `selector {` rule in the sheet,
 *  comments stripped. Mirrors StationFinderPanel.css.test.ts's helper. */
function ruleBlock(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start, `rule ${selector} exists`).toBeGreaterThanOrEqual(0);
  const end = css.indexOf('}', start);
  return css.slice(start, end).replace(/\/\*[\s\S]*?\*\//g, '');
}

describe('.si-strip__body--live fixed live-body height (tuxlink-6i0ie)', () => {
  it('declares a fixed height so the waterfall/feed row cannot grow the strip', () => {
    const block = ruleBlock('.si-strip__body--live');
    expect(block, 'evidence-measured fixed height: fits the waterfall\'s natural ~197px content')
      .toMatch(/height:\s*200px/);
  });

  it('keeps the BARE .si-strip__body height-free (setup arms need their growth budget)', () => {
    // Fix round 1 regression guard: an unconditional height on the bare rule
    // clamps NonLiveBody's setup arms too (see the module doc above). The
    // bare rule's `min-height: 0` is fine; a bare `height:` is the defect.
    const block = ruleBlock('.si-strip__body');
    expect(block, 'no bare height on .si-strip__body (only the --live modifier carries it)')
      .not.toMatch(/(?<!min-|max-)height:/);
  });
});

describe.each(['.si-wf', '.si-feed'])('%s bounded flex child (tuxlink-6i0ie)', (selector) => {
  it('declares min-height: 0 so it can be constrained by the fixed-height row', () => {
    const block = ruleBlock(selector);
    expect(block).toMatch(/min-height:\s*0/);
  });
});

describe('.si-feed scroll containment (tuxlink-6i0ie)', () => {
  it('keeps an overflow declaration (auto or hidden) now that it has a bounded height', () => {
    const block = ruleBlock('.si-feed');
    expect(block).toMatch(/overflow:\s*(auto|hidden)/);
  });
});
