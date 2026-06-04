import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useMermaidRender } from './useMermaidRender';
import { resetMermaidLoaderForTesting } from './mermaidLoader';

// jsdom does not implement SVGElement.getBBox, which Mermaid's flowchart
// renderer calls during layout.  Mock the loader so `loadMermaid()` returns a
// stub that resolves immediately with a fake SVG string, letting the hook's
// promise chain and DOM replacement logic be tested without hitting the jsdom
// limitation.
vi.mock('./mermaidLoader', () => ({
  loadMermaid: vi.fn().mockResolvedValue({
    render: vi.fn().mockResolvedValue({ svg: '<svg data-testid="mermaid-svg"></svg>' }),
  }),
  resetMermaidLoaderForTesting: vi.fn(),
}));

describe('useMermaidRender', () => {
  beforeEach(() => resetMermaidLoaderForTesting());

  it('replaces mermaid code block with rendered SVG', async () => {
    const container = document.createElement('div');
    container.innerHTML = '<pre><code class="language-mermaid">graph TD\nA-->B</code></pre>';

    renderHook(() => useMermaidRender({ current: container }, container.innerHTML));

    await waitFor(() => {
      expect(container.innerHTML).toContain('<svg');
    }, { timeout: 3000 });
  });

  it('no-ops on a container with no mermaid blocks', async () => {
    const container = document.createElement('div');
    container.innerHTML = '<p>no mermaid here</p>';
    const original = container.innerHTML;

    renderHook(() => useMermaidRender({ current: container }, original));

    // Wait briefly, confirm no change
    await new Promise(r => setTimeout(r, 100));
    expect(container.innerHTML).toBe(original);
  });

  it('re-renders mermaid when the content signal changes (tuxlink-f95k)', async () => {
    // Regression for tuxlink-f95k: switching topics after first mount must
    // re-scan + re-render mermaid blocks. Keying the effect on containerRef
    // alone misses this because the ref identity is stable across renders.
    const container = document.createElement('div');

    // First render: no mermaid block. Hook mounts but does nothing.
    container.innerHTML = '<p>topic A — no mermaid</p>';
    const { rerender } = renderHook(
      ({ signal }: { signal: string }) =>
        useMermaidRender({ current: container }, signal),
      { initialProps: { signal: container.innerHTML } },
    );

    // Simulate a topic switch: the container now holds a mermaid block.
    container.innerHTML = '<pre><code class="language-mermaid">graph TD\nC-->D</code></pre>';
    rerender({ signal: container.innerHTML });

    await waitFor(() => {
      expect(container.innerHTML).toContain('<svg');
    }, { timeout: 3000 });
  });
});
