import { describe, it, expect } from 'vitest';

// CSS-string assertions for the FZ-M1 compact mode (tuxlink-h7q7). jsdom cannot
// compute layout or evaluate media queries, so we raw-import the stylesheet and
// assert on the @media block as a string — the established pattern in
// AppShell.compact.test.tsx. Uses import.meta.glob ?raw (no node:fs).
const WIZARD_CSS_MODULES = import.meta.glob('./wizard.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

const css = WIZARD_CSS_MODULES['./wizard.css'];
const COMPACT = '@media (max-width: 1365px)';

describe('wizard desktop regression guard (tuxlink-h7q7)', () => {
  it('uses the strict 1365px breakpoint (NOT 1366) so desktop >=1366px is untouched', () => {
    expect(css).toContain(COMPACT);
    expect(css).not.toContain('max-width: 1366px');
  });

  it('keeps the desktop card padding + 580px max-width bare (unscoped)', () => {
    const beforeBlock = css.slice(0, css.indexOf(COMPACT));
    // The original desktop declarations must still exist OUTSIDE the media query.
    expect(beforeBlock).toContain('max-width: 580px');
    expect(beforeBlock).toContain('padding: 38px 40px 34px');
    expect(beforeBlock).toContain('padding: clamp(32px, 8vh, 96px) 24px 48px');
  });
});

describe('wizard compact CSS contract (tuxlink-h7q7)', () => {
  const block = css.slice(css.indexOf(COMPACT));

  it('preserves the 580px centered card (no width override in compact)', () => {
    // The card width is NOT touched in compact — only padding/density change.
    expect(block).not.toContain('max-width: 580px');
  });

  it('lowers the wizard-root top padding (density, not the desktop clamp)', () => {
    expect(block).toMatch(
      /\.wizard-root \{\s*padding:\s*clamp\(16px, 3vh, 32px\) 24px 48px;/,
    );
  });

  it('reduces card padding 38/40 -> 24/28', () => {
    expect(block).toMatch(/\.wizard-step \{\s*padding:\s*24px 28px 24px;/);
  });

  it('caps the session log to 30vh and lifts its mono font off the 12px floor', () => {
    expect(block).toMatch(
      /\.wizard-session-log \{\s*max-height:\s*30vh;\s*font-size:\s*13px;/,
    );
  });

  it('bumps the failed-detail mono font to 13px', () => {
    expect(block).toMatch(/\.wizard-failed-detail \{\s*font-size:\s*13px;/);
  });
});

describe('wizard compact touch targets (>=44px)', () => {
  const block = css.slice(css.indexOf(COMPACT));

  it('pins submit-row buttons to >=44px', () => {
    expect(block).toMatch(
      /\.wizard-submit-row > button \{\s*min-height:\s*44px;/,
    );
  });

  it('pins text inputs to >=44px', () => {
    expect(block).toMatch(/\.wizard-field input \{\s*min-height:\s*44px;/);
  });

  it('pins the password show/hide toggle to >=44px', () => {
    expect(block).toMatch(
      /\.wizard-password-row button \{\s*min-height:\s*44px;/,
    );
  });

  it('pins the link-button ("Go to inbox now") to >=44px', () => {
    expect(block).toMatch(/\.wizard-btn-link[\s\S]{0,120}min-height:\s*44px;/);
  });

  it('pins the error-banner Retry button to >=44px', () => {
    expect(block).toMatch(
      /\.wizard-error-banner button \{\s*min-height:\s*44px;/,
    );
  });
});

describe('wizard compact font floors + legibility', () => {
  const block = css.slice(css.indexOf(COMPACT));

  it('lifts sub-floor 12/12.5px offenders to 13px (field label, transport-visibility, mock-banner, footer)', () => {
    expect(block).toContain('.wizard-field label');
    expect(block).toContain('.wizard-transport-visibility');
    expect(block).toContain('.wizard-mock-banner');
    expect(block).toContain('.wizard-footer-copy');
    // The shared font-floor rule sets 13px.
    expect(block).toMatch(/\.wizard-footer-copy \{\s*\n?\s*font-size:\s*13px;/);
  });

  it('nudges inline code 0.88em -> 0.92em', () => {
    expect(block).toMatch(/code \{\s*font-size:\s*0\.92em;/);
  });

  it('lightens the faint footer color one step to --text-dim', () => {
    expect(block).toMatch(
      /\.wizard-footer-copy \{\s*color:\s*var\(--text-dim\);/,
    );
  });
});

describe('wizard compact — inline Register anchor is NOT restyled (design call)', () => {
  const block = css.slice(css.indexOf(COMPACT));

  it('does not speculatively give the inline anchor a block/min-height (flagged for design)', () => {
    // The inline Register anchor (`.wizard-root a`) cannot cleanly reach 44px
    // inline; it is flagged for a design decision, NOT restyled here. Assert no
    // bare `.wizard-root a` touch-target rule leaked into the compact block.
    expect(block).not.toMatch(/\.wizard-root a \{[\s\S]*?min-height/);
  });
});
