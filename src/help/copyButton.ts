// src/help/copyButton.ts
//
// Post-render decoration that adds a "Copy" button to every <pre> code block,
// except mermaid blocks (which get replaced with SVG by useMermaidRender).
// Called from ReadingPane after the markdown HTML lands.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.

export function addCopyButtons(container: HTMLElement): void {
  const blocks = container.querySelectorAll<HTMLPreElement>('pre');
  blocks.forEach((pre) => {
    const code = pre.querySelector('code');
    if (!code) return;
    if (code.classList.contains('language-mermaid')) return;
    if (pre.querySelector('.copy-button')) return; // Idempotent — re-runs no-op.

    const btn = document.createElement('button');
    btn.className = 'copy-button';
    btn.type = 'button';
    btn.setAttribute('aria-label', 'Copy code to clipboard');
    btn.textContent = 'Copy';

    btn.addEventListener('click', () => {
      const text = code.textContent ?? '';
      navigator.clipboard?.writeText(text).then(() => {
        btn.textContent = 'Copied';
        setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
      }).catch(() => {
        btn.textContent = 'Failed';
        setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
      });
    });

    pre.insertBefore(btn, pre.firstChild);
  });
}
