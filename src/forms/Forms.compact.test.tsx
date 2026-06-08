import { describe, it, expect } from 'vitest';

// FZ-M1 compact-mode CSS-string assertions for the "HTML Forms host + viewer +
// picker" surface (tuxlink-h7q7, Phase 6). Four sub-surfaces:
//   src/forms/forms.css            — authoring + read-side form chrome (reflows)
//   src/forms/FormPicker.css       — modal listbox + action bar
//   src/compose/WebviewFormHost.css   — compose-side embedded-form chrome
//   src/mailbox/WebviewFormViewer.css — mailbox-side embedded-viewer chrome
//
// jsdom cannot compute layout or evaluate media queries, so these are
// CSS-STRING assertions: raw-import each stylesheet, slice it at the
// `@media (max-width: 1365px)` boundary, and assert the load-bearing rules
// (the ICS-309 single-col reflow, the Damage 2-col reflow, the legend-input
// 100%, the 44px touch targets, the 12px font floors) live INSIDE the compact
// block — and that desktop (everything before it) carries no compact rule, so
// >=1366px stays byte-identical. Mirrors src/shell/AppShell.compact.test.tsx;
// uses the import.meta.glob ?raw pattern (no node:fs — pitfall TEST-1).

/** Each single-file glob yields exactly one module; return its CSS string
 * without depending on the exact (possibly normalized) glob key. */
function only(mods: Record<string, string>): string {
  const values = Object.values(mods);
  expect(values).toHaveLength(1);
  return values[0];
}

// NOTE: import.meta.glob requires an OBJECT LITERAL second arg (Vite statically
// analyzes it) — a hoisted variable fails to transform.
const formsCss = only(import.meta.glob('./forms.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>);
const pickerCss = only(import.meta.glob('./FormPicker.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>);
const hostCss = only(import.meta.glob('../compose/WebviewFormHost.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>);
const viewerCss = only(import.meta.glob('../mailbox/WebviewFormViewer.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>);

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

describe('HTML forms compact CSS — breakpoint discipline (tuxlink-h7q7)', () => {
  const all = [formsCss, pickerCss, hostCss, viewerCss];

  it('uses strictly 1365px (not 1366) in every forms surface', () => {
    for (const css of all) {
      expect(css).toContain(COMPACT);
      // 1366 would overlap the desktop-invariant floor — Codex adrev R1 #1.
      expect(css).not.toContain('max-width: 1366px');
    }
  });

  it('keeps every compact touch rule inside the media query (no desktop leak)', () => {
    // No `min-height: 44px` touch rule may live in the desktop portion — those
    // all belong inside the breakpoint or desktop >=1366px changes.
    for (const css of all) {
      expect(desktopPortion(css)).not.toContain('min-height: 44px');
    }
  });
});

describe('forms.css compact — grid reflows + touch + font floors', () => {
  const block = compactBlock(formsCss);

  it('LOAD-BEARING: reflows the ICS-309 log entry 3-col grid to a single column', () => {
    expect(block).toMatch(/\.ics309-log-entry \{\s*grid-template-columns:\s*1fr;/);
  });

  it('LOAD-BEARING: collapses the Damage Assessment 6-col grid to 2 columns', () => {
    expect(block).toMatch(/\.damage-category \{\s*grid-template-columns:\s*repeat\(2, minmax\(0, 1fr\)\);/);
  });

  it('LOAD-BEARING: stretches the category-name input 200px -> full width', () => {
    expect(block).toMatch(/\.damage-category > legend > input \{\s*width:\s*100%;/);
  });

  it('pins native field inputs/textareas to the 44px touch floor', () => {
    expect(block).toMatch(/input:not\(\[type="checkbox"\]\), textarea\)[\s\S]*?min-height:\s*44px/);
  });

  it('sizes the native checkbox to 22px (per spec — full 44px not required)', () => {
    expect(block).toMatch(/input\[type="checkbox"\] \{\s*width:\s*22px;\s*height:\s*22px;/);
  });

  it('bumps the "+ Add entry" button, slot-toolbar controls, and form actions to 44px', () => {
    expect(block).toMatch(/\.ics309-form fieldset > button\[type="button"\]:not\(\.form-actions button\) \{\s*min-height:\s*44px/);
    expect(block).toMatch(/\.form-slot-toolbar select,\s*\.form-slot-toolbar button \{\s*min-height:\s*44px/);
    expect(block).toMatch(/\.form-actions button \{\s*min-height:\s*44px/);
  });

  it('raises the sub-floor fonts (field label, log-entry heading, table th) 11px -> 12px', () => {
    // The native-field label rule (shared 3-selector) carries the floor.
    expect(block).toMatch(/\.ics309-log-entry, \.damage-category\) > label \{\s*font-size:\s*12px/);
    expect(block).toMatch(/\.ics309-log-entry > strong \{\s*font-size:\s*12px/);
    expect(block).toMatch(/\.ics309-log-table th,\s*\.damage-category-table th \{\s*font-size:\s*12px/);
  });
});

describe('FormPicker.css compact — listbox rows + actions', () => {
  const block = compactBlock(pickerCss);

  it('pins the form-picker list rows to a 44px tap target', () => {
    expect(block).toMatch(/\.form-picker-list li \{\s*min-height:\s*44px/);
  });

  it('bumps the picker action buttons to 44px', () => {
    expect(block).toMatch(/\.form-picker-actions button \{\s*min-height:\s*44px/);
  });
});

describe('WebviewFormHost.css compact — OUR chrome only', () => {
  const block = compactBlock(hostCss);

  it('bumps the chrome action buttons to 44px', () => {
    expect(block).toMatch(/\.webview-form-host__btn \{\s*min-height:\s*44px/);
  });

  it('does NOT touch the embedded-webview placeholder rect (no __embed compact rule)', () => {
    // The `__embed` placeholder must stay untouched so the child webview's
    // ResizeObserver position is not disturbed (plan §R1).
    expect(block).not.toContain('webview-form-host__embed');
  });
});

describe('WebviewFormViewer.css compact — OUR chrome only', () => {
  const block = compactBlock(viewerCss);

  it('bumps the read-only Close button to 44px', () => {
    expect(block).toMatch(/\.webview-form-viewer__btn \{\s*min-height:\s*44px/);
  });

  it('does NOT touch the embedded-webview placeholder rect (no __embed compact rule)', () => {
    expect(block).not.toContain('webview-form-viewer__embed');
  });
});
