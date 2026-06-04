import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { headingAnchors } from './headingAnchors';

describe('headingAnchors extension', () => {
  function render(md: string) {
    const m = new Marked();
    m.use(headingAnchors);
    return m.parse(md) as string;
  }

  it('adds id to h1', () => {
    expect(render('# Hello World')).toContain('id="hello-world"');
  });

  it('adds id to h2 + h3', () => {
    expect(render('## Foo Bar')).toContain('id="foo-bar"');
    expect(render('### Baz Qux')).toContain('id="baz-qux"');
  });

  it('handles punctuation by stripping it', () => {
    expect(render('## VARA HF — Standard')).toContain('id="vara-hf-standard"');
  });

  it('handles multiple consecutive spaces', () => {
    expect(render('## foo  bar')).toContain('id="foo-bar"');
  });

  it('lowercases everything', () => {
    expect(render('## DigiRig')).toContain('id="digirig"');
  });

  it('preserves heading text content', () => {
    expect(render('## DigiRig')).toContain('>DigiRig</h2>');
  });

  it('strips inline link markup so the slug uses rendered text, not raw url', () => {
    expect(render('## See the [docs](https://example.com) for details'))
      .toContain('id="see-the-docs-for-details"');
  });

  it('handles CJK characters via Unicode-aware slug rules', () => {
    // The \p{Letter} class with the u flag treats CJK code points as letters,
    // not non-alphanumeric — they survive slugification.
    expect(render('## 無線局 station')).toContain('id="無線局-station"');
  });
});
