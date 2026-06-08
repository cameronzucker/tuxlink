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
