// src/shell/markdownExtensions/callouts.ts
//
// marked extension for GitHub-style callouts:
//   > [!NOTE]
//   > Body
// Renders as <div class="callout callout-note">Body</div>.
// Types: note, warning, tip, danger. Unknown types pass through as
// plain blockquotes.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.

import type { MarkedExtension, Tokens } from 'marked';

const CALLOUT_TYPES = new Set(['note', 'warning', 'tip', 'danger']);

function detectCalloutType(token: Tokens.Blockquote): string | null {
  const firstChild = token.tokens[0];
  if (!firstChild || firstChild.type !== 'paragraph') return null;
  const para = firstChild as Tokens.Paragraph;
  const firstText = para.tokens[0];
  if (!firstText || firstText.type !== 'text') return null;
  const match = (firstText as Tokens.Text).text.match(/^\[!(\w+)\]/);
  if (!match) return null;
  const type = match[1].toLowerCase();
  return CALLOUT_TYPES.has(type) ? type : null;
}

function stripCalloutMarker(token: Tokens.Blockquote): void {
  const firstChild = token.tokens[0];
  if (!firstChild || firstChild.type !== 'paragraph') return;
  const para = firstChild as Tokens.Paragraph;
  const firstText = para.tokens[0];
  if (!firstText || firstText.type !== 'text') return;
  const t = firstText as Tokens.Text;
  // Remove the [!TYPE] marker and any leading whitespace/newline.
  // Update both `text` and `raw` so downstream extensions reading either
  // field see the post-strip content. Marked's render path uses `text`,
  // but keeping `raw` consistent avoids latent surprises (e.g., position
  // tracking or a future re-lex hook).
  const stripRe = /^\[!\w+\]\s*\n?/;
  t.text = t.text.replace(stripRe, '');
  t.raw = t.raw.replace(stripRe, '');
  if (t.text === '') {
    // Drop the now-empty leading text token. The [!TYPE]-only callout
    // (no body) renders as `<div class="callout callout-X"><p></p></div>`
    // which is valid HTML; styling can `p:empty { display: none }` if
    // a layout author cares.
    para.tokens.shift();
  }
}

export const callouts: MarkedExtension = {
  renderer: {
    blockquote(token: Tokens.Blockquote): string {
      const type = detectCalloutType(token);
      if (!type) {
        // Pass through as plain blockquote — use the explicit fallback to
        // guarantee correctness across marked versions.
        return `<blockquote>\n${this.parser.parse(token.tokens)}</blockquote>\n`;
      }
      stripCalloutMarker(token);
      const inner = this.parser.parse(token.tokens);
      return `<div class="callout callout-${type}">${inner}</div>\n`;
    },
  },
};
