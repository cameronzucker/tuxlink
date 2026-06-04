import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { defLists } from './defLists';

function render(md: string): string {
  const m = new Marked();
  m.use(defLists);
  return m.parse(md) as string;
}

describe('definition lists extension', () => {
  it('renders a single term + definition', () => {
    const out = render('Term\n:   Definition.');
    expect(out).toContain('<dl>');
    expect(out).toContain('<dt>Term</dt>');
    expect(out).toContain('<dd>Definition.</dd>');
    expect(out).toContain('</dl>');
  });

  it('renders multiple terms in one list', () => {
    const out = render('B2F\n:   Block Forwarding 2.\n\nCMS\n:   Common Message Server.');
    expect(out.match(/<dl>/g)?.length).toBe(1);
    expect(out.match(/<dt>/g)?.length).toBe(2);
    expect(out).toContain('<dt>B2F</dt>');
    expect(out).toContain('<dt>CMS</dt>');
  });

  it('regular paragraphs unaffected', () => {
    expect(render('Just a paragraph.')).toContain('<p>Just a paragraph.</p>');
  });
});
