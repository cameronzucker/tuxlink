import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useMermaidRender } from './useMermaidRender';
import { loadMermaid, resetMermaidLoaderForTesting } from './mermaidLoader';

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

  it('pins explicit width/height from the viewBox and drops inline max-width (tuxlink-3xnf)', async () => {
    // Regression for tuxlink-3xnf: Mermaid v11 emits
    // `<svg width="100%" style="max-width: Npx" viewBox="0 0 W H">`. WebKitGTK
    // stretches width="100%" to the full pane and scales the height off the
    // aspect ratio, rendering the diagram enormously and clipping foreignObject
    // node labels. The hook must pin intrinsic width/height from the viewBox and
    // remove Mermaid's inline max-width so the CSS max-width:100% can downscale.
    vi.mocked(loadMermaid).mockResolvedValueOnce({
      render: vi.fn().mockResolvedValue({
        svg: '<svg width="100%" style="max-width: 342.5px;" viewBox="0 0 342.5 564"></svg>',
      }),
    } as never);

    const container = document.createElement('div');
    container.innerHTML = '<pre><code class="language-mermaid">flowchart TD\nA-->B</code></pre>';

    renderHook(() => useMermaidRender({ current: container }, container.innerHTML));

    await waitFor(() => {
      const svg = container.querySelector('svg');
      expect(svg).not.toBeNull();
      expect(svg!.getAttribute('width')).toBe('342.5');
      expect(svg!.getAttribute('height')).toBe('564');
      // the WebKit-stretch trigger (width="100%" + inline max-width) is gone
      expect(svg!.getAttribute('width')).not.toBe('100%');
      expect(svg!.style.maxWidth).toBe('');
    }, { timeout: 3000 });
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
