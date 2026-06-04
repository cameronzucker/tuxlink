// src/shell/sanitizeHtml.ts
//
// DOMPurify wrapper with the project's allowed-element policy. Wrap any
// HTML that originated from markdown (or any other authored content)
// through this before injecting via dangerouslySetInnerHTML.
//
// Allowed elements: the markdown-relevant subset (headings, paragraphs,
// lists, links, code, images, tables, callout div, definition lists,
// footnote sup/a/section, blockquote). All others stripped.
//
// Allowed attributes: id, class, href, src, alt, title. All others stripped.
// In particular, all event handlers (onclick, onload, etc.) are stripped.
//
// Forbidden tags: script, style, iframe, object, embed, form, input.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.4.

import DOMPurify from 'dompurify';

const ALLOWED_TAGS = [
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'p', 'br', 'hr',
  'ul', 'ol', 'li',
  'dl', 'dt', 'dd',
  'strong', 'em', 'code', 'pre',
  'a', 'img',
  'table', 'thead', 'tbody', 'tr', 'th', 'td',
  'blockquote',
  'div',  // callout wrapper
  'sup', 'sub',  // footnotes
  'section',  // footnotes container
  'span',
];

const ALLOWED_ATTR = [
  'id', 'class',
  'href', 'src', 'alt', 'title',
  // Footnote rel=noopener/noreferrer is added by marked-footnote
  'rel',
];

const FORBIDDEN_TAGS = ['script', 'style', 'iframe', 'object', 'embed', 'form', 'input', 'button'];

export function sanitizeHtml(dirty: string): string {
  return DOMPurify.sanitize(dirty, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    FORBID_TAGS: FORBIDDEN_TAGS,
    USE_PROFILES: { html: true },
  });
}
