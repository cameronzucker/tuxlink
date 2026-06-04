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

export function useMermaidRender(containerRef: RefObject<HTMLElement | null>): void {
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
          pre.replaceWith(wrapper);
        }).catch((err) => {
          console.error('Mermaid render failed:', err);
        });
      });
    });

    return () => {
      cancelled = true;
    };
  }, [containerRef]);
}
