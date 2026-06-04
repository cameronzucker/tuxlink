// src/shell/markdownRenderV2.ts
//
// Replacement renderer for the tuxlink help window. Wraps `marked` with the
// Tier 3 extension chain (heading anchors, callouts, tables, footnotes, def
// lists) and emits HTML string. Sanitization happens in sanitizeHtml.ts;
// callers route output through there before injecting via
// dangerouslySetInnerHTML. Spec: docs/superpowers/specs/2026-06-03-docs-
// knowledge-base-design.md §4.

import { Marked } from 'marked';
import markedExtendedTables from 'marked-extended-tables';
import markedFootnote from 'marked-footnote';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';
import { defLists } from './markdownExtensions/defLists';
import { imageResolver } from './markdownExtensions/imageResolver';

// Bundle all docs/user-guide images at build time. Vite's import.meta.glob
// with { eager: true, query: '?url' } returns { '/docs/.../foo.png': '/assets/foo-hash.png' }.
const IMAGE_MAPPING = import.meta.glob('/docs/user-guide/images/**/*.{png,svg,jpg,jpeg,webp}', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);
marked.use(markedExtendedTables());
marked.use(markedFootnote());
marked.use(defLists);
marked.use(imageResolver(IMAGE_MAPPING));

/**
 * Parse a markdown string and return HTML.
 *
 * The output is NOT sanitized — pass it through `sanitizeHtml` from
 * `./sanitizeHtml.ts` before rendering via dangerouslySetInnerHTML.
 */
export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
