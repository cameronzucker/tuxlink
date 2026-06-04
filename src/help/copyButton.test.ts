import { describe, it, expect, vi } from 'vitest';
import { addCopyButtons } from './copyButton';

describe('addCopyButtons', () => {
  it('adds a copy button to every <pre>', () => {
    const container = document.createElement('div');
    container.innerHTML = '<pre><code>x = 1</code></pre><pre><code>y = 2</code></pre>';
    addCopyButtons(container);
    expect(container.querySelectorAll('.copy-button').length).toBe(2);
  });

  it('does not add button to mermaid blocks (they get replaced with SVG)', () => {
    const container = document.createElement('div');
    container.innerHTML = '<pre><code class="language-mermaid">graph TD</code></pre>';
    addCopyButtons(container);
    expect(container.querySelectorAll('.copy-button').length).toBe(0);
  });

  it('clicking the button copies the code text to clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const container = document.createElement('div');
    container.innerHTML = '<pre><code>x = 1</code></pre>';
    addCopyButtons(container);

    const btn = container.querySelector('.copy-button') as HTMLButtonElement;
    btn.click();

    expect(writeText).toHaveBeenCalledWith('x = 1');
  });
});
