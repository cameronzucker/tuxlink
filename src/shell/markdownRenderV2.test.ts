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
});
