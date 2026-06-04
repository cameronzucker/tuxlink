import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { imageResolver } from './imageResolver';

function render(md: string, mapping: Record<string, string>): string {
  const m = new Marked();
  m.use(imageResolver(mapping));
  return m.parse(md) as string;
}

describe('image path resolver', () => {
  it('rewrites a relative image path to bundler URL', () => {
    const mapping = { '/docs/user-guide/images/10-digirig/front.png': '/assets/front-deadbeef.png' };
    const md = '![DigiRig front](images/10-digirig/front.png)';
    const out = render(md, mapping);
    expect(out).toContain('src="/assets/front-deadbeef.png"');
    expect(out).toContain('alt="DigiRig front"');
  });

  it('leaves absolute URLs untouched', () => {
    const out = render('![X](https://example.com/x.png)', {});
    expect(out).toContain('src="https://example.com/x.png"');
  });

  it('warns on unresolved relative paths (in test env: throws)', () => {
    const md = '![X](images/nonexistent.png)';
    expect(() => render(md, {})).toThrow(/unresolved image/i);
  });
});
