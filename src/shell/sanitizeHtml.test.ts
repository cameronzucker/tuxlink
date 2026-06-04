import { describe, it, expect } from 'vitest';
import { sanitizeHtml } from './sanitizeHtml';

describe('sanitizeHtml', () => {
  it('strips <script>', () => {
    expect(sanitizeHtml('<p>ok</p><script>alert(1)</script>')).not.toContain('<script>');
    expect(sanitizeHtml('<p>ok</p><script>alert(1)</script>')).toContain('<p>ok</p>');
  });

  it('strips inline event handlers', () => {
    const out = sanitizeHtml('<a href="x" onclick="evil()">link</a>');
    expect(out).not.toContain('onclick');
    expect(out).toContain('href="x"');
  });

  it('strips <iframe>', () => {
    expect(sanitizeHtml('<iframe src="evil"></iframe>')).not.toContain('iframe');
  });

  it('strips <style>', () => {
    expect(sanitizeHtml('<style>* { color: red; }</style>')).not.toContain('<style>');
  });

  it('allows heading + p + ul + li', () => {
    const md = '<h2 id="x">H</h2><p>P</p><ul><li>I</li></ul>';
    const out = sanitizeHtml(md);
    expect(out).toContain('<h2');
    expect(out).toContain('id="x"');
    expect(out).toContain('<p>P</p>');
    expect(out).toContain('<ul>');
  });

  it('allows callout div with allowlist classes', () => {
    const md = '<div class="callout callout-note">body</div>';
    expect(sanitizeHtml(md)).toContain('class="callout callout-note"');
  });

  it('allows <img> with src + alt + title', () => {
    const md = '<img src="/x.png" alt="x" title="t">';
    const out = sanitizeHtml(md);
    expect(out).toContain('src="/x.png"');
    expect(out).toContain('alt="x"');
  });

  it('allows table elements', () => {
    expect(sanitizeHtml('<table><tr><td>x</td></tr></table>')).toContain('<table>');
  });

  it('allows <pre><code class="language-bash">', () => {
    const out = sanitizeHtml('<pre><code class="language-bash">echo</code></pre>');
    expect(out).toContain('language-bash');
  });

  it('allows <dl><dt><dd>', () => {
    expect(sanitizeHtml('<dl><dt>T</dt><dd>D</dd></dl>')).toContain('<dt>');
  });

  it('allows footnote <sup> + back-link <a>', () => {
    const md = '<sup><a href="#fn1" id="fnref1">1</a></sup>';
    expect(sanitizeHtml(md)).toContain('<sup>');
  });
});
