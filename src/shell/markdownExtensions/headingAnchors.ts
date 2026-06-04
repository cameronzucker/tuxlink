// src/shell/markdownExtensions/headingAnchors.ts
//
// marked extension that adds an `id` attribute to every heading, slugified
// from the heading text. Enables FTS5 deep-linking and in-topic anchor
// navigation (see ReadingPane's extended link interceptor).
//
// Slug rules:
// - Lowercase
// - Replace runs of non-alphanumeric with single hyphen
// - Strip leading/trailing hyphens
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.

import type { MarkedExtension, Tokens } from 'marked';

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

export const headingAnchors: MarkedExtension = {
  renderer: {
    heading(token: Tokens.Heading): string {
      const text = token.text;
      const id = slugify(text);
      const inner = this.parser.parseInline(token.tokens);
      return `<h${token.depth} id="${id}">${inner}</h${token.depth}>\n`;
    },
  },
};
