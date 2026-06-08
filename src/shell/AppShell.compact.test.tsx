import { describe, it, expect } from 'vitest';

// IMPORTANT (Codex adrev R1 #7): a Vite `?raw` import of AppShell.css does NOT
// inline `@import './compactShell.css'` — it returns the literal import line. So
// we raw-import BOTH files and assert on them separately. desktopCss
// (AppShell.css) holds the untouched desktop rules; compactCss (compactShell.css)
// holds the @media block. Uses the same import.meta.glob ?raw pattern as
// AppShell.test.tsx (no node:fs — pitfall TEST-1).
const DESKTOP_CSS_MODULES = import.meta.glob('./AppShell.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const COMPACT_CSS_MODULES = import.meta.glob('./compactShell.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const COMPACT_DRAWER_CSS = import.meta.glob('./RadioDrawer.css', {
  eager: true, query: '?raw', import: 'default',
}) as Record<string, string>;

const desktopCss = DESKTOP_CSS_MODULES['./AppShell.css'];
const compactCss = COMPACT_CSS_MODULES['./compactShell.css'];
const COMPACT = '@media (max-width: 1365px)';

describe('AppShell desktop regression guard (tuxlink-h7q7)', () => {
  it('keeps the desktop panes grid templates in AppShell.css, unscoped (no compact @media)', () => {
    // The desktop file must NOT contain any non-print compact media query.
    expect(desktopCss).not.toContain('max-width: 1365px');
    // The three desktop templates exist as bare (un-media-scoped) rules.
    expect(desktopCss).toContain('grid-template-columns: 200px 380px 1fr');
    expect(desktopCss).toContain('grid-template-columns: 200px 380px 1fr 400px');
  });
});

describe('AppShell compact CSS contract (tuxlink-h7q7)', () => {
  it('puts every compact layout rule inside the 1365px breakpoint (compactShell.css)', () => {
    expect(compactCss).toContain(COMPACT);
    // Nothing layout-bearing may live before the media query. Only comments and
    // the desktop `.rail-expand-btn { display:none }` hide rule are allowed
    // outside (the latter hides the expand toggle at desktop).
    const beforeBlock = compactCss
      .slice(0, compactCss.indexOf(COMPACT))
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/\.rail-expand-btn\s*\{[^}]*\}/g, '')
      .trim();
    expect(beforeBlock).toBe('');
  });
});

describe('Compact panes grid (tuxlink-813d — overlay drawer replaces push)', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  it('uses a 48px rail + 3-column base (reader keeps full 1fr — no 4th column)', () => {
    // The compact panes grid is exactly 3 columns; no 4th column for the drawer.
    expect(block).toMatch(/\.layout-b \.panes \{\s*grid-template-columns:\s*48px 380px 1fr;/);
  });
  // The old push-drawer 4th-column tests (44px grip / 400px open / drawer-open
  // grid template / panes--with-legacy-dock column) are intentionally removed in
  // tuxlink-813d D1. The overlay tests in the FZ-M1 overlay drawer block below
  // are the new guards.
});

describe('Compact icon rail (Task 10) — a11y-safe label hide', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  it('clips labels (keeps them in the a11y tree) instead of display:none (F1)', () => {
    // The nav-label rule must use clip-path, NOT display:none.
    expect(block).toMatch(/\.nav-label[\s\S]{0,400}clip-path:\s*inset\(50%\)/);
    expect(block).not.toMatch(/\.nav-label[^}]*display:\s*none/);
  });
  it('pins rail rows to >=44px and gives an inset focus ring (F5)', () => {
    expect(block).toMatch(/\.sidebar \.nav-item \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.nav-item:focus-visible \{[\s\S]*?outline-offset:\s*-2px/);
  });
  it('expands to an overlay (absolute), not a push reflow (open item 4)', () => {
    expect(block).toMatch(/\.sidebar\.is-expanded \{[\s\S]*?position:\s*absolute/);
  });
});

describe('Compact ribbon + chrome (Task 11)', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  it('fixes the ribbon clip (smaller search-zone, connection, gap)', () => {
    expect(block).toMatch(/\.search-zone \{\s*flex:\s*0 0 360px/);
    expect(block).toMatch(/\.dash-connection \{\s*max-width:\s*180px/);
  });
  it('raises the worst sub-floor chrome font (.dash-source-segment 9px → 12px)', () => {
    expect(block).toMatch(/\.dash-source-segment \{\s*font-size:\s*12px/);
  });
  it('bumps titlebar controls to a 44px touch target', () => {
    expect(block).toMatch(/\.tux-titlebar \.tux-ctrl \{[\s\S]*?min-width:\s*44px/);
  });
});

describe('FZ-M1 overlay drawer (tuxlink-813d)', () => {
  it('compact panes grid no longer reserves a 4th radio column', () => {
    const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
    expect(compactBlock).not.toContain('48px 380px 1fr 44px');
    expect(compactBlock).not.toContain('48px 380px 1fr 400px');
    expect(compactBlock).not.toContain('52px 380px 1fr 44px');
    expect(compactBlock).not.toContain('52px 380px 1fr 400px');
  });
  it('compact radio drawer is an absolute overlay, not a grid column', () => {
    const drawerCss = COMPACT_DRAWER_CSS['./RadioDrawer.css'];
    const compactBlock = drawerCss.slice(drawerCss.indexOf('max-width: 1365px'));
    expect(compactBlock).toContain('position: absolute');
  });
});
