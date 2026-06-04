// src/shell/markdownExtensions/imageResolver.ts
//
// marked extension that rewrites <img src> relative paths to bundler-resolved
// URLs. The path mapping comes from import.meta.glob in the production
// renderer; tests inject explicit maps.
//
// Paths starting with http(s):// or // are absolute and pass through untouched.
// Relative paths must be prefixed with `images/` and resolve under
// `/docs/user-guide/images/`. Unresolved relative paths throw — operators see
// them at build time, not at first-runtime page view.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.6.

import type { MarkedExtension, Tokens } from 'marked';

const ABSOLUTE_RE = /^(?:https?:)?\/\//;

export function imageResolver(mapping: Record<string, string>): MarkedExtension {
  return {
    renderer: {
      image(token: Tokens.Image): string {
        const { href, title, text } = token;
        let resolved = href;
        if (!ABSOLUTE_RE.test(href)) {
          const key = `/docs/user-guide/${href}`;
          const url = mapping[key];
          if (!url) {
            throw new Error(`Unresolved image reference: ${href} (looked up as ${key})`);
          }
          resolved = url;
        }
        const altAttr = ` alt="${escape(text)}"`;
        const titleAttr = title ? ` title="${escape(title)}"` : '';
        return `<img src="${escape(resolved)}"${altAttr}${titleAttr}>`;
      },
    },
  };
}

function escape(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
