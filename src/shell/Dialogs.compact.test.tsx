import { describe, it, expect } from 'vitest';

// FZ-M1 compact-mode CSS-string assertions for the three modal dialogs
// (Settings / Theme / About). tuxlink-h7q7, Phase 4 / Task 14.
//
// jsdom cannot evaluate media queries or compute layout, so — exactly like
// AppShell.compact.test.tsx — these are CSS-STRING assertions: raw-import each
// stylesheet, slice it at the @media index, and assert the load-bearing rules
// live inside the compact block (and that NOTHING compact leaks before it, which
// would change desktop >=1366px). Uses the same import.meta.glob ?raw pattern as
// AppShell.compact.test.tsx (no node:fs — pitfall TEST-1).
const CSS_MODULES = import.meta.glob(
  ['./SettingsPanel.css', './ThemeDesigner.css', './AboutDialog.css'],
  { eager: true, query: '?raw', import: 'default' },
) as Record<string, string>;

const settingsCss = CSS_MODULES['./SettingsPanel.css'];
const themeCss = CSS_MODULES['./ThemeDesigner.css'];
const aboutCss = CSS_MODULES['./AboutDialog.css'];

const COMPACT = '@media (max-width: 1365px)';

/** The portion of `css` at/after the compact @media block. */
function compactBlock(css: string): string {
  const idx = css.indexOf(COMPACT);
  expect(idx).toBeGreaterThanOrEqual(0); // the block must exist
  return css.slice(idx);
}

/** The portion of `css` BEFORE the compact @media block (desktop rules). */
function desktopPart(css: string): string {
  const idx = css.indexOf(COMPACT);
  return css.slice(0, idx);
}

describe('Dialogs compact CSS — breakpoint discipline (tuxlink-h7q7)', () => {
  it('uses the strict 1365px breakpoint (not 1366) in every dialog stylesheet', () => {
    for (const css of [settingsCss, themeCss, aboutCss]) {
      expect(css).toContain(COMPACT);
      // 1366 would overlap the desktop-invariant floor — Codex adrev R1 #1.
      expect(css).not.toContain('max-width: 1366px');
    }
  });

  it('keeps the desktop swatch at 36x28 (the compact 44x44 must not leak up)', () => {
    // Desktop portion still declares the original swatch size.
    expect(desktopPart(themeCss)).toMatch(/\.tux-theme-designer-color \{[\s\S]*?width:\s*36px;[\s\S]*?height:\s*28px;/);
  });
});

describe('Theme dialog compact — swatch + inputs + density (tuxlink-h7q7)', () => {
  const block = compactBlock(themeCss);

  it('PRIMARY: bumps the color swatch (x24) to a 44x44 touch target', () => {
    expect(block).toMatch(/\.tux-theme-designer-color \{\s*width:\s*44px;\s*height:\s*44px;/);
  });

  it('raises hex + name/select inputs to the 44px touch floor', () => {
    expect(block).toMatch(/\.tux-theme-designer-color-text \{[\s\S]*?min-height:\s*44px/);
    // The two <select>s and the name field all reuse .tux-theme-designer-text-input.
    expect(block).toMatch(/\.tux-theme-designer-text-input \{\s*min-height:\s*44px;/);
  });

  it('bumps action buttons to 44px', () => {
    expect(block).toMatch(/\.tux-theme-designer-button \{\s*min-height:\s*44px;/);
  });

  it('tightens group padding 14 -> 12 to offset the taller rows', () => {
    expect(block).toMatch(/\.tux-theme-designer-group \{\s*padding:\s*12px 18px;/);
  });

  it('raises sub-floor help fonts to 13px', () => {
    expect(block).toMatch(/font-size:\s*13px/);
  });
});

describe('DRY close button — ONE rule, all three modals (tuxlink-h7q7)', () => {
  it('lives in SettingsPanel.css as a single multi-selector rule at 44x44', () => {
    const block = compactBlock(settingsCss);
    expect(block).toMatch(
      /\.tux-settings-close,\s*\.tux-theme-designer-close,\s*\.tux-about-close \{[\s\S]*?min-width:\s*44px;[\s\S]*?min-height:\s*44px;/,
    );
  });

  it('is NOT duplicated in the other two dialog stylesheets', () => {
    // The DRY contract: ThemeDesigner.css and AboutDialog.css must not restate
    // their own close-button compact rule.
    expect(compactBlock(themeCss)).not.toContain('tux-theme-designer-close');
    expect(compactBlock(aboutCss)).not.toContain('tux-about-close {');
  });
});

describe('Settings dialog compact — opt rows + native radio (tuxlink-h7q7)', () => {
  const block = compactBlock(settingsCss);

  it('pins each option row to a 44px tap target', () => {
    expect(block).toMatch(/\.tux-settings-opt \{\s*min-height:\s*44px;/);
  });

  it('bumps the native radio control', () => {
    expect(block).toMatch(/\.tux-settings-opt input\[type='radio'\] \{[\s\S]*?width:\s*20px;[\s\S]*?height:\s*20px;/);
  });

  it('raises opt-help 11 -> 13', () => {
    expect(block).toMatch(/\.tux-settings-opt-help \{\s*font-size:\s*13px;/);
  });
});

describe('About dialog compact — meta links (worst touch targets) (tuxlink-h7q7)', () => {
  const block = compactBlock(aboutCss);

  it('gives the 5 inline meta links an inline-block padded 44px box', () => {
    expect(block).toMatch(
      /\.tux-about-meta a \{[\s\S]*?display:\s*inline-block;[\s\S]*?min-height:\s*44px;/,
    );
  });

  it('opens the inter-row gap 6 -> 10 to separate adjacent links', () => {
    expect(block).toMatch(/\.tux-about-meta \{[\s\S]*?row-gap:\s*10px;/);
  });

  it('raises about-meta 12 -> 13', () => {
    expect(block).toMatch(/\.tux-about-meta \{[\s\S]*?font-size:\s*13px;/);
  });
});
