// src/shell/markdownRenderV2.ts
//
// Replacement renderer for the tuxlink help window. Wraps `marked` with the
// Tier 3 extension chain (heading anchors, callouts, tables, footnotes, def
// lists) and emits HTML string. Sanitization happens in sanitizeHtml.ts;
// callers route output through there before injecting via
// dangerouslySetInnerHTML. Spec: docs/superpowers/specs/2026-06-03-docs-
// knowledge-base-design.md §4.

import { Marked } from 'marked';
import { headingAnchors } from './markdownExtensions/headingAnchors';

const marked = new Marked();
marked.use(headingAnchors);

/**
 * Parse a markdown string and return HTML.
 *
 * The output is NOT sanitized — pass it through `sanitizeHtml` from
 * `./sanitizeHtml.ts` before rendering via dangerouslySetInnerHTML.
 */
export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
