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
