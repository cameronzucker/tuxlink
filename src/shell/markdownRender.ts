// Minimal markdown → JSX block parser for the Help panel (tuxlink-35g0).
//
// Why hand-rolled, not `marked` or `react-markdown`: bundling a markdown
// engine adds ~40-100 KB and broader supply-chain surface for a UI panel
// whose content is bundled at build time. The user-guide markdown subset
// uses only headings (h1–h3), paragraphs, unordered lists, fenced code
// blocks, plus inline bold / italic / code / links. The hand-rolled parser
// covers exactly that and nothing else.
//
// The renderer is a pure function: markdown string → array of typed blocks.
// React consumption lives in src/help/ReadingPane.tsx; this module knows nothing about
// the DOM, which lets the parser be tested in isolation.

export interface InlineText {
  /** Linear sequence of inline runs. Each run carries its formatting. */
  runs: InlineRun[];
}

export type InlineRun =
  | { kind: 'text'; text: string }
  | { kind: 'bold'; text: string }
  | { kind: 'italic'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'link'; text: string; href: string };

export type Block =
  | { kind: 'heading'; level: 1 | 2 | 3; text: InlineText }
  | { kind: 'paragraph'; text: InlineText }
  | { kind: 'list'; items: InlineText[] }
  | { kind: 'code'; lang: string | null; text: string }
  | { kind: 'table'; headers: InlineText[]; rows: InlineText[][] };

/** Parse one line of inline text into formatted runs. The order of matches
 *  matters: code (`...`) before bold (`**...**`) before italic (`_..._`)
 *  before links (`[text](url)`) so backticks don't get re-formatted. */
export function parseInline(line: string): InlineText {
  const runs: InlineRun[] = [];
  let cursor = 0;

  // Order: code, link, bold, italic. Code first so backticks aren't subject
  // to inner formatting. Link before bold because `[**bold link**](url)`
  // is fine but `**[link](url)**` is awkward (and rare in the bundled docs).
  const patterns: { re: RegExp; build: (m: RegExpExecArray) => InlineRun }[] = [
    { re: /`([^`]+)`/, build: (m) => ({ kind: 'code', text: m[1] }) },
    { re: /\[([^\]]+)\]\(([^)]+)\)/, build: (m) => ({ kind: 'link', text: m[1], href: m[2] }) },
    { re: /\*\*([^*]+)\*\*/, build: (m) => ({ kind: 'bold', text: m[1] }) },
    { re: /_([^_]+)_/, build: (m) => ({ kind: 'italic', text: m[1] }) },
  ];

  while (cursor < line.length) {
    const rest = line.slice(cursor);
    // Find the earliest match across all patterns; tie-break by source order
    // (so the patterns list above is also the priority list).
    let best: { idx: number; len: number; run: InlineRun } | null = null;
    for (const p of patterns) {
      const m = p.re.exec(rest);
      if (!m) continue;
      if (best === null || m.index < best.idx) {
        best = { idx: m.index, len: m[0].length, run: p.build(m) };
      }
    }
    if (!best) {
      runs.push({ kind: 'text', text: rest });
      break;
    }
    if (best.idx > 0) {
      runs.push({ kind: 'text', text: rest.slice(0, best.idx) });
    }
    runs.push(best.run);
    cursor += best.idx + best.len;
  }

  return { runs };
}

/** Parse a markdown string into a sequence of typed blocks. */
export function parseMarkdown(source: string): Block[] {
  const lines = source.split(/\r?\n/);
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Skip blank lines between blocks.
    if (line.trim() === '') {
      i++;
      continue;
    }

    // Fenced code block.
    if (line.startsWith('```')) {
      const lang = line.slice(3).trim() || null;
      const start = i + 1;
      let end = start;
      while (end < lines.length && !lines[end].startsWith('```')) {
        end++;
      }
      const text = lines.slice(start, end).join('\n');
      blocks.push({ kind: 'code', lang, text });
      i = end + 1;
      continue;
    }

    // Headings (#, ##, ###).
    const h = /^(#{1,3})\s+(.+)$/.exec(line);
    if (h) {
      const level = h[1].length as 1 | 2 | 3;
      blocks.push({ kind: 'heading', level, text: parseInline(h[2]) });
      i++;
      continue;
    }

    // Table: a row of pipe-separated cells, the next line is the alignment
    // separator (`|---|---|`), the lines that follow are body rows. The
    // minimal parser only handles plain pipe tables — no alignment, no
    // multi-line cells.
    if (
      line.includes('|') &&
      i + 1 < lines.length &&
      /^\s*\|?\s*:?-{2,}.*\|/.test(lines[i + 1])
    ) {
      const cells = (s: string) =>
        s
          .replace(/^\s*\|/, '')
          .replace(/\|\s*$/, '')
          .split('|')
          .map((c) => c.trim());

      const headers = cells(line).map(parseInline);
      let rowStart = i + 2;
      const rows: InlineText[][] = [];
      while (rowStart < lines.length && lines[rowStart].includes('|') && lines[rowStart].trim() !== '') {
        rows.push(cells(lines[rowStart]).map(parseInline));
        rowStart++;
      }
      blocks.push({ kind: 'table', headers, rows });
      i = rowStart;
      continue;
    }

    // Unordered list. Each item starts with "- "; continuation lines
    // (indented, no bullet marker) are joined into the current item.
    // tuxlink-ew3k bug 5: the prior loop stopped on the first non-bullet
    // line, leaving "- foo\n  bar" as one list item ("foo") plus an
    // orphan paragraph ("bar") — visible at the bottom of 07-settings.md
    // where the second link wraps across two source lines.
    if (/^\s*-\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length) {
        const cur = lines[i];
        if (/^\s*-\s+/.test(cur)) {
          // New item.
          items.push(cur.replace(/^\s*-\s+/, ''));
          i++;
        } else if (items.length > 0 && /^\s+\S/.test(cur)) {
          // Continuation of the current item: indented, non-empty.
          items[items.length - 1] += ' ' + cur.trim();
          i++;
        } else {
          break;
        }
      }
      blocks.push({ kind: 'list', items: items.map(parseInline) });
      continue;
    }

    // Paragraph: collect consecutive non-blank, non-heading, non-list lines.
    const buf: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !/^#{1,3}\s+/.test(lines[i]) &&
      !/^\s*-\s+/.test(lines[i]) &&
      !lines[i].startsWith('```')
    ) {
      buf.push(lines[i]);
      i++;
    }
    if (buf.length > 0) {
      blocks.push({ kind: 'paragraph', text: parseInline(buf.join(' ')) });
    }
  }

  return blocks;
}
