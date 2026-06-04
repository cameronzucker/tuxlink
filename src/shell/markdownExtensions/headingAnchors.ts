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
      // Use the rendered inline HTML stripped of tags as the slug source.
      // Using `token.text` directly leaks raw markdown syntax (e.g. link URLs
      // inside `[text](url)`) into the anchor id. The rendered-text approach
      // produces stable slugs across markdown-formatting variations.
      const inner = this.parser.parseInline(token.tokens);
      const text = inner.replace(/<[^>]+>/g, '');
      const id = slugify(text);
      return `<h${token.depth} id="${id}">${inner}</h${token.depth}>\n`;
    },
  },
};
