// src/help/useMermaidRender.ts
//
// React hook that observes a container, finds <pre><code class="language-mermaid">
// blocks, lazy-loads Mermaid, and replaces each block's contents with rendered
// SVG. Designed to be called from ReadingPane after dangerouslySetInnerHTML.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.3.
//
// Security note: the SVG injected via innerHTML originates from Mermaid's own
// renderer operating on the help-doc source (trusted, not user-supplied).
// DOMPurify sanitization runs on the outer markdown content before it reaches
// this hook; Mermaid itself is a trusted renderer (not user data passthrough).
// The securityLevel:'strict' option in mermaidLoader.ts additionally prevents
// script injection within diagram definitions.

import { useEffect, type RefObject } from 'react';
import { loadMermaid } from './mermaidLoader';

let renderCounter = 0;

/**
 * Pin a rendered Mermaid SVG to its intrinsic pixel size.
 *
 * Mermaid v11 emits `<svg width="100%" style="max-width: Npx" viewBox="0 0 W H">`
 * with no height attribute (its `useMaxWidth` default). Chromium derives the
 * intrinsic size from the viewBox and honors the inline max-width, so the
 * diagram renders at its natural W×H. WebKitGTK — the production webview — does
 * NOT: it stretches `width="100%"` to the full container width and scales the
 * height off the aspect ratio, rendering simple flowcharts enormously, and its
 * foreignObject-in-scaled-SVG handling then clips the HTML node labels. Both
 * symptoms, one cause (tuxlink-3xnf).
 *
 * Setting explicit intrinsic `width`/`height` from the viewBox and dropping
 * Mermaid's inline `max-width` makes WebKit render at natural size; the CSS rule
 * `.mermaid-diagram svg { max-width: 100%; height: auto }` then scales the
 * diagram DOWN responsively on panes narrower than its natural width.
 *
 * Verified against real Mermaid 11.15.0 output (dev/scratch/mermaid-probe). The
 * visual result must be confirmed in a WebKitGTK render (grim) — jsdom/Chromium
 * cannot reproduce the WebKit sizing bug.
 */
export function normalizeMermaidSvgSize(root: ParentNode): void {
  const svgEl = root.querySelector('svg');
  if (!svgEl) return;
  const viewBox = svgEl.getAttribute('viewBox');
  if (!viewBox) return;
  const [, , w, h] = viewBox.split(/\s+/).map(Number);
  if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) return;
  svgEl.setAttribute('width', String(w));
  svgEl.setAttribute('height', String(h));
  svgEl.style.removeProperty('max-width');
}

export function useMermaidRender(
  containerRef: RefObject<HTMLElement | null>,
  contentSignal: string,
): void {
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const codeBlocks = container.querySelectorAll<HTMLElement>('pre code.language-mermaid');
    if (codeBlocks.length === 0) return;

    let cancelled = false;

    loadMermaid().then((mermaid) => {
      if (cancelled) return;
      codeBlocks.forEach((codeEl) => {
        const pre = codeEl.parentElement;
        if (!pre) return;
        const source = codeEl.textContent ?? '';
        const id = `mermaid-${++renderCounter}`;
        mermaid.render(id, source).then(({ svg }) => {
          if (cancelled) return;
          const wrapper = document.createElement('div');
          wrapper.className = 'mermaid-diagram';
          // svg is Mermaid-generated markup from trusted help-doc source;
          // securityLevel:'strict' in mermaidLoader sanitizes diagram input.
          wrapper.innerHTML = svg;
          normalizeMermaidSvgSize(wrapper);
          pre.replaceWith(wrapper);
        }).catch((err) => {
          console.error('Mermaid render failed:', err);
        });
      });
    });

    return () => {
      cancelled = true;
    };
    // contentSignal is the rendered HTML string. Keying the effect on it
    // ensures topic-switches re-scan + re-render mermaid blocks; keying on
    // containerRef alone would only fire on mount (the ref identity is stable).
  }, [containerRef, contentSignal]);
}
