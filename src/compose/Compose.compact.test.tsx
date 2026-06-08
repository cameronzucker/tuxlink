import { describe, it, expect } from 'vitest';

// FZ-M1 compact-mode CSS-string assertions (tuxlink-h7q7).
//
// jsdom cannot compute layout or evaluate media queries, so the compact rules
// are verified by importing each stylesheet as a raw string, slicing at the
// `@media (max-width: 1365px)` boundary, and asserting the load-bearing rules
// live inside the block (and that desktop — everything before it — carries no
// compact rule). Mirrors src/shell/AppShell.compact.test.tsx; uses the
// import.meta.glob ?raw pattern (no node:fs — pitfall TEST-1).

// NOTE: import.meta.glob requires an OBJECT LITERAL second arg (Vite statically
// analyzes it) — a hoisted variable fails to transform.
const COMPOSE = import.meta.glob('./Compose.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;
const CHECKIN = import.meta.glob('./CheckInForm.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;
const POSITION = import.meta.glob('./PositionFormV2.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;
const ICS309 = import.meta.glob('./Ics309FormV2.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;

const composeCss = COMPOSE['./Compose.css'];
const checkinCss = CHECKIN['./CheckInForm.css'];
const positionCss = POSITION['./PositionFormV2.css'];
const ics309Css = ICS309['./Ics309FormV2.css'];

const COMPACT = '@media (max-width: 1365px)';

/** The portion of `css` at/after the compact media query (its open brace on). */
function compactBlock(css: string): string {
  const i = css.indexOf(COMPACT);
  expect(i, `${COMPACT} must exist`).toBeGreaterThan(-1);
  return css.slice(i);
}

/** Everything before the compact media query, comments stripped. */
function desktopPortion(css: string): string {
  return css.slice(0, css.indexOf(COMPACT)).replace(/\/\*[\s\S]*?\*\//g, '');
}

describe('Compose compact CSS — breakpoint discipline (tuxlink-h7q7)', () => {
  it('uses strictly 1365px (not 1366) in every compose surface', () => {
    for (const css of [composeCss, checkinCss, positionCss, ics309Css]) {
      expect(css).toContain(COMPACT);
      expect(css).not.toContain('max-width: 1366px');
    }
  });

  it('keeps every compact rule inside the media query (no desktop leak)', () => {
    // The desktop portion (pre-@media) must contain no compact min-height /
    // 44px touch rule — those all belong inside the breakpoint.
    for (const css of [composeCss, checkinCss, positionCss, ics309Css]) {
      expect(desktopPortion(css)).not.toContain('min-height: 44px');
    }
  });
});

describe('Compose window compact rules (Compose.css)', () => {
  const block = compactBlock(composeCss);

  it('scopes the titlebar 44x44 bump to .tux-compose-titlebar (R4 — never the bare .tux-ctrl)', () => {
    expect(block).toMatch(
      /\.tux-compose-titlebar \.tux-ctrl \{[\s\S]*?min-width:\s*44px;[\s\S]*?min-height:\s*44px;/,
    );
    // The bare `.tux-ctrl` (shell-owned) must NOT be targeted unscoped here —
    // i.e. no rule selector that begins (after leading whitespace) with the
    // bare `.tux-ctrl {`. The compose rule is `.tux-compose-titlebar .tux-ctrl`.
    expect(block).not.toMatch(/^\s*\.tux-ctrl\s*\{/m);
  });

  it('bumps action buttons + text inputs to >=44px touch targets', () => {
    expect(block).toMatch(/\.compose-btn \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.compose-input \{[\s\S]*?min-height:\s*44px/);
  });

  it('gives the request-receipt checkbox a 44px label row + a 24px box', () => {
    expect(block).toMatch(/\.compose-checkbox-label \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(
      /\.compose-checkbox-label input\[type='checkbox'\] \{[\s\S]*?width:\s*24px;[\s\S]*?height:\s*24px;/,
    );
  });

  it('grows the attachments drop-zone from 36px to 48px', () => {
    expect(block).toMatch(/\.compose-attachments \{[\s\S]*?min-height:\s*48px/);
  });

  it('raises the .compose-hint sub-floor font 11px -> 12px', () => {
    expect(block).toMatch(/\.compose-hint \{[\s\S]*?font-size:\s*12px/);
  });

  it('does NOT shrink the .compose-root 14px root font (ICS-309 is rem-based)', () => {
    // No compact rule may set a font-size on the compose root.
    expect(block).not.toMatch(/\.compose-root \{[\s\S]*?font-size/);
  });
});

describe('CheckIn embedded-form compact rules (CheckInForm.css)', () => {
  const block = compactBlock(checkinCss);

  it('density: tightens the card padding 16px -> 10px', () => {
    expect(block).toMatch(/\.checkin-form \{[\s\S]*?padding:\s*10px/);
  });

  it('bumps inputs, radio rows, and action buttons to touch size', () => {
    expect(block).toMatch(/\.checkin-form input\[type='text'\][\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.checkin-form__radios label \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(
      /\.checkin-form__radios input\[type='radio'\] \{[\s\S]*?width:\s*24px/,
    );
    expect(block).toMatch(/\.checkin-form__actions button \{[\s\S]*?min-height:\s*44px/);
  });
});

describe('Position embedded-form compact rules (PositionFormV2.css)', () => {
  const block = compactBlock(positionCss);

  it('density: tightens the card padding 16px -> 10px', () => {
    expect(block).toMatch(/\.position-form-v2 \{[\s\S]*?padding:\s*10px/);
  });

  it('bumps the shared slot toolbar + inputs + actions to touch size', () => {
    expect(block).toMatch(/\.form-slot-toolbar select,[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.position-form-v2 input\[type='text'\][\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.position-form-v2__actions button \{[\s\S]*?min-height:\s*44px/);
  });

  it('raises the fix-badge AND the load-bearing grid-error font 11px -> 12px', () => {
    expect(block).toMatch(/\.position-form-v2__fix-badge \{[\s\S]*?font-size:\s*12px/);
    expect(block).toMatch(/\.position-form-v2__grid-error \{[\s\S]*?font-size:\s*12px/);
  });
});

describe('ICS-309 embedded-form compact rules (Ics309FormV2.css)', () => {
  const block = compactBlock(ics309Css);

  it('sizes the datetime-local pickers for touch + pins a 12px font floor', () => {
    expect(block).toMatch(
      /\.ics309-form-v2__custom-range input\[type="datetime-local"\] \{[\s\S]*?min-height:\s*44px;[\s\S]*?font-size:\s*12px;/,
    );
  });

  it('bumps query/preset/action buttons to >=44px', () => {
    expect(block).toMatch(/\.ics309-form-v2__query-btn,[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.ics309-form-v2__actions button \{[\s\S]*?min-height:\s*44px/);
  });

  it('CRITICAL: never shrinks the root/form font — ICS-309 sizes in rem', () => {
    // No compact rule may set a font-size on the form root, :root, or html.
    expect(block).not.toMatch(/\.ics309-form-v2 \{[\s\S]*?font-size/);
    expect(block).not.toMatch(/:root\b[\s\S]*?font-size/);
    expect(block).not.toMatch(/\bhtml\b[\s\S]*?font-size/);
  });
});
