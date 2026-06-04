// src/shell/markdownExtensions/defLists.ts
//
// marked extension for definition lists in PHP-Markdown-Extra style:
//   Term
//   :   Definition body.
// Renders as <dl><dt>Term</dt><dd>Definition body.</dd></dl>. Consecutive
// term/definition blocks (with or without blank lines between entries) merge
// into one <dl>.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.
// Used heavily in topic 30 (glossary).

import type { MarkedExtension, TokenizerExtension, RendererExtension } from 'marked';

interface DefListToken {
  type: 'defList';
  raw: string;
  entries: { term: string; definition: string }[];
}

// Match a block of one or more term/definition pairs. A blank line between
// consecutive entries is allowed (and consumed) as long as the next line
// continues with another term (not a colon-prefixed line).
// Pattern per entry: <term-line>\n:   <def-line>
// Between entries: optional single blank line followed immediately by another entry.
const DEF_LIST_RE =
  /^(?:[^\n:][^\n]*\n:\s+[^\n]+(?:\n\n(?=[^\n:][^\n]*\n:\s+))?)+/;

// Extract individual term/definition pairs from the matched block.
const ENTRY_RE = /^([^\n:][^\n]*)\n:\s+([^\n]+)/gm;

const defListTokenizer: TokenizerExtension = {
  name: 'defList',
  level: 'block',
  start(src: string): number | void {
    const m = src.match(/^[^\n:][^\n]*\n:\s+/m);
    return m?.index;
  },
  tokenizer(src: string): DefListToken | undefined {
    const match = src.match(DEF_LIST_RE);
    if (!match) return undefined;
    const block = match[0];
    const entries: { term: string; definition: string }[] = [];
    let m;
    ENTRY_RE.lastIndex = 0;
    while ((m = ENTRY_RE.exec(block)) !== null) {
      entries.push({
        term: m[1].trim(),
        definition: m[2].trim().replace(/\n\s+/g, ' '),
      });
    }
    if (entries.length === 0) return undefined;
    return { type: 'defList', raw: block, entries };
  },
};

const defListRenderer: RendererExtension = {
  name: 'defList',
  renderer(token): string {
    const t = token as DefListToken;
    const items = t.entries
      .map((e) => `<dt>${escapeHtml(e.term)}</dt><dd>${escapeHtml(e.definition)}</dd>`)
      .join('');
    return `<dl>${items}</dl>\n`;
  },
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export const defLists: MarkedExtension = {
  extensions: [defListTokenizer as never, defListRenderer as never],
};
