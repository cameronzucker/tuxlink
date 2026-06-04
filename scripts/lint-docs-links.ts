// scripts/lint-docs-links.ts
//
// Build-time linter for the docs/user-guide/ link graph. Verifies:
// - Every cross-topic .md ref points at an existing topic file.
// - Every #anchor ref points at an existing heading in the relevant file.
// - No .md ref escapes docs/user-guide/ (no ../pitfalls/, no /etc.).
//
// Runs via `pnpm lint:docs` and in CI / pre-push.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §5.8.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

interface LinterError {
  file: string;
  href: string;
  reason: string;
}

interface LinterInput {
  files: Record<string, string>;
}

interface LinterResult {
  errors: LinterError[];
}

const LINK_RE = /\[[^\]]*\]\(([^)]+)\)/g;
const HEADING_RE = /^#+\s+(.+)$/gm;
const MD_REF_RE = /^(?:.*\/)?(\d{2}-[a-z0-9-]+)\.md(?:#([\w-]+))?$/;
const ANCHOR_ONLY_RE = /^#([\w-]+)$/;
const OUT_OF_BUNDLE_RE = /^(?:\.\.\/|\/)/;
const HTTP_RE = /^https?:\/\//;

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

function extractAnchorsForFile(content: string): Set<string> {
  const anchors = new Set<string>();
  let m;
  HEADING_RE.lastIndex = 0;
  while ((m = HEADING_RE.exec(content)) !== null) {
    anchors.add(slugify(m[1].trim()));
  }
  return anchors;
}

export function lintMarkdownLinks(input: LinterInput): LinterResult {
  const errors: LinterError[] = [];
  const anchorsByFile = new Map<string, Set<string>>();
  const topicSlugs = new Set<string>();

  for (const [path, content] of Object.entries(input.files)) {
    anchorsByFile.set(path, extractAnchorsForFile(content));
    const fname = path.split('/').pop() || '';
    const slugMatch = fname.match(/^(\d{2}-[a-z0-9-]+)\.md$/);
    if (slugMatch) topicSlugs.add(slugMatch[1]);
  }

  for (const [path, content] of Object.entries(input.files)) {
    let m;
    LINK_RE.lastIndex = 0;
    while ((m = LINK_RE.exec(content)) !== null) {
      const href = m[1].trim();

      if (HTTP_RE.test(href)) continue; // External — not validated by this linter.

      if (OUT_OF_BUNDLE_RE.test(href)) {
        errors.push({
          file: path,
          href,
          reason: 'links outside the user-guide bundle are not allowed',
        });
        continue;
      }

      const anchorOnly = href.match(ANCHOR_ONLY_RE);
      if (anchorOnly) {
        const anchor = anchorOnly[1];
        const anchors = anchorsByFile.get(path) ?? new Set();
        if (!anchors.has(anchor)) {
          errors.push({ file: path, href, reason: 'anchor target does not exist in this topic' });
        }
        continue;
      }

      const mdRef = href.match(MD_REF_RE);
      if (mdRef) {
        const targetSlug = mdRef[1];
        const targetAnchor = mdRef[2];
        if (!topicSlugs.has(targetSlug)) {
          errors.push({ file: path, href, reason: 'target topic does not exist' });
          continue;
        }
        if (targetAnchor) {
          const targetPath = `docs/user-guide/${targetSlug}.md`;
          const targetAnchors = anchorsByFile.get(targetPath) ?? new Set();
          if (!targetAnchors.has(targetAnchor)) {
            errors.push({ file: path, href, reason: 'anchor target does not exist in cross-topic file' });
          }
        }
        continue;
      }
    }
  }

  return { errors };
}

export function lintFromDisk(root: string): LinterResult {
  const files: Record<string, string> = {};
  function walk(dir: string) {
    for (const entry of readdirSync(dir)) {
      const path = join(dir, entry);
      const st = statSync(path);
      if (st.isDirectory()) walk(path);
      else if (entry.endsWith('.md')) {
        files[path] = readFileSync(path, 'utf8');
      }
    }
  }
  walk(root);
  return lintMarkdownLinks({ files });
}

// CLI entry point
if (import.meta.url === `file://${process.argv[1]}`) {
  const result = lintFromDisk('docs/user-guide');
  if (result.errors.length === 0) {
    console.log('✓ Link linter passed.');
    process.exit(0);
  } else {
    console.error(`✗ Link linter found ${result.errors.length} errors:`);
    for (const e of result.errors) {
      console.error(`  ${e.file}: [${e.href}] — ${e.reason}`);
    }
    process.exit(1);
  }
}
