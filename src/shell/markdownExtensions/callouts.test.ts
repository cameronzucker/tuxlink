import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { callouts } from './callouts';

function render(md: string): string {
  const m = new Marked();
  m.use(callouts);
  return m.parse(md) as string;
}

describe('callouts extension', () => {
  it('renders [!NOTE] callout', () => {
    const out = render('> [!NOTE]\n> Body.');
    expect(out).toContain('class="callout callout-note"');
    expect(out).toContain('Body.');
  });

  it('renders [!WARNING] callout', () => {
    expect(render('> [!WARNING]\n> Beware.')).toContain('class="callout callout-warning"');
  });

  it('renders [!TIP] callout', () => {
    expect(render('> [!TIP]\n> Hint.')).toContain('class="callout callout-tip"');
  });

  it('renders [!DANGER] callout', () => {
    expect(render('> [!DANGER]\n> Stop.')).toContain('class="callout callout-danger"');
  });

  it('preserves multi-line body', () => {
    const out = render('> [!NOTE]\n> Line 1.\n> Line 2.');
    expect(out).toContain('Line 1.');
    expect(out).toContain('Line 2.');
  });

  it('regular blockquotes pass through unchanged', () => {
    const out = render('> Just a quote.');
    expect(out).toContain('<blockquote');
    expect(out).not.toContain('callout');
  });

  it('unknown callout type passes through as plain blockquote', () => {
    expect(render('> [!UNKNOWN]\n> body')).toContain('<blockquote');
  });
});
