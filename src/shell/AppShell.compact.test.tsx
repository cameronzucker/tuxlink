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
  it('uses a 52px vertical-tab rail + 3-column base (reader keeps full 1fr — no 4th column)', () => {
    // The compact panes grid is exactly 3 columns; no 4th column for the drawer.
    // tuxlink-813d D2/D3: the rail widened 48→52px for the vertical-text tabs.
    expect(block).toMatch(/\.layout-b \.panes \{\s*grid-template-columns:\s*52px 380px 1fr;/);
  });
  // The old push-drawer 4th-column tests (44px grip / 400px open / drawer-open
  // grid template / panes--with-legacy-dock column) are intentionally removed in
  // tuxlink-813d D1. The overlay tests in the FZ-M1 overlay drawer block below
  // are the new guards.
});

describe('Compact rail (tuxlink-813d D2/D3) — vertical-text tabs + flyout', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  // tuxlink-813d D2: the indistinct icon rail is replaced with vertical-text
  // tabs (bottom-to-top, Outlook-spine). The a11y-clip label-hide rules and the
  // `.sidebar.is-expanded` absolute mutation are GONE (D3 structural fix): the
  // rail never leaves the grid; the expanded nav is a separate `.sidebar-flyout`.
  it('compact rail uses bottom-to-top vertical-text tabs', () => {
    expect(block).toContain('writing-mode: vertical-rl');
    expect(block).toContain('rotate(180deg)'); // bottom-to-top reading
    expect(block).toContain('grid-template-columns: 52px 380px 1fr');
  });
  it('expanded rail does NOT make .sidebar position:absolute (grid stays intact)', () => {
    // Root fix for problem 2 (grid implosion): the rail never goes absolute.
    expect(block).not.toMatch(/\.sidebar\.is-expanded\s*\{[^}]*position:\s*absolute/);
    // The expanded nav is its own absolutely-positioned flyout element.
    expect(block).toContain('.sidebar-flyout');
  });
  it('pins vertical-tab rows to >=44px and gives an inset focus ring', () => {
    expect(block).toMatch(/\.sidebar \.vtab \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.vtab:focus-visible \{[\s\S]*?outline-offset:\s*-2px/);
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

describe('Compact ribbon GridEdit alignment (Task 5, tuxlink-813d)', () => {
  it('compact ribbon aligns the GridEdit source cluster', () => {
    const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
    // The source-segment control class must be targeted inside the compact block.
    expect(compactBlock).toContain('.dash-source-segment');
    // The grid item cluster must have an alignment hook inside compact.
    expect(compactBlock).toMatch(/align-items:\s*center/);
    // The GridEdit item must collapse its gap so label+44px touch target fits in 56px ribbon.
    expect(compactBlock).toMatch(/\.dash-item--grid[\s\S]*?gap:\s*0/);
    // The dash-grid-display inner flex must also align its children.
    expect(compactBlock).toMatch(/\.dash-item--grid \.dash-grid-display[\s\S]*?align-items:\s*center/);
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

describe('FZ-M1 operator smoke fixes (tuxlink-813d)', () => {
  // All three sub-describes below read the same compactCss / drawerCss that the
  // existing guards use — no new imports needed.
  const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
  const drawerCss = COMPACT_DRAWER_CSS['./RadioDrawer.css'];
  const drawerCompactBlock = drawerCss.slice(drawerCss.indexOf('max-width: 1365px'));

  describe('ribbon fixes (#2)', () => {
    // tuxlink-813d smoke #2: the grid value button was flex-squeezed to ~31px and
    // `white-space: normal` wrapped "· CN87" onto two lines. Fixed by pinning it
    // to one line at its intrinsic width.
    it('pins .dash-grid-value-btn to white-space: nowrap', () => {
      // The rule must exist inside the compact block near .dash-grid-value-btn.
      expect(compactBlock).toContain('.layout-b .dashboard .dash-grid-value-btn');
      expect(compactBlock).toContain('white-space: nowrap');
    });

    // tuxlink-813d smoke #2: the verbose GPS-no-fix status + "Set manually" link
    // ballooned the Grid cell to 378px. Hidden in compact; desktop-only.
    it('hides .dash-gps-no-fix-status and .dash-set-manually in compact', () => {
      expect(compactBlock).toContain('.dash-gps-no-fix-status');
      expect(compactBlock).toContain('.dash-set-manually');
      // Both are inside a shared rule block that sets display:none.
      const gpsIdx = compactBlock.indexOf('.dash-gps-no-fix-status');
      // Find the next closing brace after the selector — should contain display:none.
      const ruleEnd = compactBlock.indexOf('}', gpsIdx);
      const rule = compactBlock.slice(gpsIdx, ruleEnd + 1);
      expect(rule).toContain('display: none');
    });

    // tuxlink-813d smoke #2: top-aligning all ribbon cells so the inherently
    // taller Grid cell (44px touch segment) doesn't drag the labels off-center.
    it('top-aligns the compact dashboard flex container (align-items: flex-start)', () => {
      // The rule lives on .ribbon-with-search .dashboard inside the compact block.
      expect(compactBlock).toContain('align-items: flex-start');
      // Belt-and-suspenders: the padding-top accompanies the alignment fix.
      expect(compactBlock).toContain('padding-top: 9px');
    });

    // #2b (operator full-screen grim): with the SSID picker + GPS segment, the
    // callsign/grid value areas are 44px while the clock/connection are short
    // text, so the values sat ~12px apart. Every value area gets a uniform 44px
    // centered band so all values share one baseline.
    it('gives every value area a uniform 44px centered band', () => {
      expect(compactBlock).toContain('.dash-item > .dash-value');
      const idx = compactBlock.indexOf('.dash-item > .dash-value');
      const rule = compactBlock.slice(idx, idx + 220);
      expect(rule).toContain('min-height: 44px');
      expect(rule).toMatch(/align-items:\s*center/);
    });
  });

  describe('grip enlargement (#1b)', () => {
    // tuxlink-813d smoke #1b: the old 16px-wide grip sliver read as clipped /
    // unclickable. Fixed to 30px, z-index:9 (above ResizeHandles), with a visible
    // chevron span.
    it('grip is 30px wide in the compact block', () => {
      expect(drawerCompactBlock).toContain('width: 30px');
    });

    it('grip has z-index: 9 so it sits above the ResizeHandles', () => {
      expect(drawerCompactBlock).toContain('z-index: 9');
    });

    it('grip renders a .radio-drawer-grip-chevron span', () => {
      // The CSS class must be defined inside the compact block so the chevron
      // is a styled element in compact.
      expect(drawerCompactBlock).toContain('.radio-drawer-grip-chevron');
    });
  });
});
