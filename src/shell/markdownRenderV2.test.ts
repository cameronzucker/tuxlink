import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdownRenderV2';

describe('renderMarkdown', () => {
  it('renders a heading to <h1>', () => {
    expect(renderMarkdown('# Hello')).toContain('<h1');
    expect(renderMarkdown('# Hello')).toContain('>Hello</h1>');
  });

  it('renders a paragraph', () => {
    expect(renderMarkdown('Hello world.')).toContain('<p>Hello world.</p>');
  });

  it('renders unordered lists', () => {
    expect(renderMarkdown('- one\n- two')).toMatch(/<ul>[\s\S]*<li>one<\/li>[\s\S]*<li>two<\/li>[\s\S]*<\/ul>/);
  });

  it('renders fenced code blocks', () => {
    const out = renderMarkdown('```\nx = 1\n```');
    expect(out).toContain('<pre>');
    expect(out).toContain('<code');
    expect(out).toContain('x = 1');
  });
});

describe('tables', () => {
  it('renders a pipe-delimited table', () => {
    const md = '| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |';
    const out = renderMarkdown(md);
    expect(out).toContain('<table');
    expect(out).toContain('<th>A</th>');
    expect(out).toContain('<td>1</td>');
    expect(out).toContain('<td>4</td>');
  });

  it('renders a multi-line cell via extended tables', () => {
    // marked-extended-tables supports cells with line breaks via <br> escape
    const md = '| A | B |\n|---|---|\n| line1<br>line2 | x |';
    const out = renderMarkdown(md);
    expect(out).toContain('line1');
    expect(out).toContain('line2');
  });

  it('renders rowspan via marked-extended-tables (proves the extension is wired)', () => {
    // Rowspan syntax per marked-extended-tables README: insert `^` immediately
    // before the closing pipe of any cell that should merge with the cell above.
    // A two-row span produces rowspan="2" on the first cell.
    // This test FAILS when the extension is not wired (native marked does not
    // produce rowspan attributes — it renders each row independently).
    const md = '| H1           | H2      |\n|--------------|----------|\n| spans two    | Cell A  |\n| rows        ^| Cell B  |';
    const out = renderMarkdown(md);
    // The extension emits rowspan=2 (unquoted) — match either form to be
    // forward-compatible, but the key assertion is that "rowspan" is present at all.
    expect(out).toMatch(/rowspan="?2"?/);
  });
});

describe('footnotes', () => {
  it('renders inline ref + footnote body', () => {
    const md = 'See note.[^1]\n\n[^1]: The footnote body.';
    const out = renderMarkdown(md);
    expect(out).toMatch(/sup.*1/);
    expect(out).toContain('The footnote body.');
  });

  it('produces back-link from footnote body to inline ref', () => {
    const md = 'See[^a].\n\n[^a]: Body.';
    const out = renderMarkdown(md);
    // marked-footnote emits href="#footnote-ref-<label>" as the back-link
    // (not "#fnref" — the actual pattern the library produces)
    expect(out).toMatch(/href="#footnote-ref-/);
  });
});
