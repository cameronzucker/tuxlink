// Tests for the minimal markdown → block parser (tuxlink-35g0).
//
// The parser only covers the markdown features the bundled user-guide uses:
// headings, paragraphs, unordered lists, fenced code blocks, simple tables,
// and inline bold / italic / code / links.

import { describe, it, expect } from 'vitest';
import { parseInline, parseMarkdown, type Block } from './markdownRender';

describe('parseInline', () => {
  it('returns a single text run for plain text', () => {
    const t = parseInline('hello world');
    expect(t.runs).toEqual([{ kind: 'text', text: 'hello world' }]);
  });

  it('parses inline code', () => {
    const t = parseInline('press `Ctrl+N` to open');
    expect(t.runs).toEqual([
      { kind: 'text', text: 'press ' },
      { kind: 'code', text: 'Ctrl+N' },
      { kind: 'text', text: ' to open' },
    ]);
  });

  it('parses bold', () => {
    const t = parseInline('the **important** part');
    expect(t.runs).toEqual([
      { kind: 'text', text: 'the ' },
      { kind: 'bold', text: 'important' },
      { kind: 'text', text: ' part' },
    ]);
  });

  it('parses italic', () => {
    const t = parseInline('this is _emphasised_');
    expect(t.runs).toEqual([
      { kind: 'text', text: 'this is ' },
      { kind: 'italic', text: 'emphasised' },
    ]);
  });

  it('parses links', () => {
    const t = parseInline('see [the docs](docs/01.md) for more');
    expect(t.runs).toEqual([
      { kind: 'text', text: 'see ' },
      { kind: 'link', text: 'the docs', href: 'docs/01.md' },
      { kind: 'text', text: ' for more' },
    ]);
  });

  it('handles backticks before bold (priority order)', () => {
    // The parser scans for the earliest match. A code span before a bold
    // span keeps the code span intact even though bold also appears in
    // the same line.
    const t = parseInline('`code` then **bold**');
    expect(t.runs).toEqual([
      { kind: 'code', text: 'code' },
      { kind: 'text', text: ' then ' },
      { kind: 'bold', text: 'bold' },
    ]);
  });
});

describe('parseMarkdown', () => {
  it('parses an H1 + paragraph', () => {
    const blocks = parseMarkdown('# Title\n\nHello world.');
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toMatchObject({ kind: 'heading', level: 1 });
    expect(blocks[1]).toMatchObject({ kind: 'paragraph' });
  });

  it('parses heading levels h1/h2/h3', () => {
    const blocks = parseMarkdown('# A\n\n## B\n\n### C');
    expect(blocks.map((b) => (b.kind === 'heading' ? b.level : null))).toEqual([1, 2, 3]);
  });

  it('joins multi-line paragraphs into a single block', () => {
    const blocks = parseMarkdown('First line.\nSecond line.\n\nThird.');
    expect(blocks).toHaveLength(2);
    expect((blocks[0] as Block & { kind: 'paragraph' }).text.runs[0]).toEqual({
      kind: 'text',
      text: 'First line. Second line.',
    });
  });

  it('parses unordered lists', () => {
    const blocks = parseMarkdown('- a\n- b\n- c');
    expect(blocks).toHaveLength(1);
    const list = blocks[0] as Block & { kind: 'list' };
    expect(list.kind).toBe('list');
    expect(list.items).toHaveLength(3);
  });

  it('parses fenced code blocks (with and without lang)', () => {
    const blocks = parseMarkdown('```bash\nrm -rf foo\n```\n\n```\nfree text\n```');
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toEqual({ kind: 'code', lang: 'bash', text: 'rm -rf foo' });
    expect(blocks[1]).toEqual({ kind: 'code', lang: null, text: 'free text' });
  });

  it('parses pipe tables', () => {
    const src = '| Shortcut | Action |\n|---|---|\n| Ctrl+N | New |\n| F5 | Connect |';
    const blocks = parseMarkdown(src);
    expect(blocks).toHaveLength(1);
    const table = blocks[0] as Block & { kind: 'table' };
    expect(table.kind).toBe('table');
    expect(table.headers).toHaveLength(2);
    expect(table.rows).toHaveLength(2);
  });

  it('skips blank lines between blocks', () => {
    const blocks = parseMarkdown('# A\n\n\n\nB');
    expect(blocks).toHaveLength(2);
  });
});
