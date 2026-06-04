// src/help/mermaidLoader.ts
//
// Lazy-loads Mermaid the first time it's needed. Subsequent calls reuse the
// same module instance via memoized promise. Mermaid is ~250 KB minified;
// the help window's first paint must not block on its load.
//
// Initialization is theme-aware — themeVariables pull from CSS custom
// properties so diagrams adopt the active color scheme.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.3.

type MermaidApi = typeof import('mermaid').default;

let mermaidPromise: Promise<MermaidApi> | null = null;

export function loadMermaid(): Promise<MermaidApi> {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((mod) => {
      const m = mod.default;
      // themeVariables are intentionally omitted from initialize() because
      // Mermaid v11 runs khroma color-parsing on them at init time, which
      // rejects CSS custom-property strings (e.g. "var(--color-surface)").
      // Color theming is applied at render time via CSS instead; the app's
      // stylesheet overrides mermaid's SVG variables after the diagram is
      // injected into the DOM.
      m.initialize({
        startOnLoad: false,
        theme: 'base',
        securityLevel: 'strict',
      });
      return m;
    });
  }
  return mermaidPromise;
}

/** Test-only: clear the memoized promise so each test starts fresh. */
export function resetMermaidLoaderForTesting(): void {
  mermaidPromise = null;
}
