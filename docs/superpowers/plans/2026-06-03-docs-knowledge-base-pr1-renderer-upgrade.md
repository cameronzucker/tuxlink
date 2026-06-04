# Docs Knowledge Base PR #1 — Renderer Upgrade + IA Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled `markdownRender.ts` with `marked` + Tier 3 extensions, sanitize the rendered HTML via DOMPurify, restructure the existing 10 user-guide topics into the new 32-topic information architecture, scaffold the 23 new stub topics, add a build-time link linter, and update the help-window infrastructure to consume HTML output. No new content shipped — the IA is in place but new topics are stubs.

**Architecture:** Replace the JSX `Block[]` renderer (current `src/shell/markdownRender.ts`) with `marked`-emitted HTML rendered via `dangerouslySetInnerHTML` after DOMPurify sanitization. Extend the `ReadingPane.tsx` link interceptor to handle in-topic anchors. Mermaid loaded lazily on first use. Build-time linter walks the `docs/user-guide/` tree for link integrity. The existing test surface around `markdownRender` is replaced with HTML-output assertions.

**Tech Stack:** TypeScript, React, [`marked`](https://marked.js.org/), [`marked-extended-tables`](https://github.com/calculuschild/marked-extended-tables), [`marked-footnote`](https://github.com/bent10/marked-extensions), [`DOMPurify`](https://github.com/cure53/DOMPurify), [`Mermaid`](https://mermaid.js.org/), Vitest, Vite (`import.meta.glob`).

**Spec:** `docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md` (commit f27c3ff on branch bd-tuxlink-ymiv/docs-knowledge-base-spec).

**bd issue ecosystem:** Parent `tuxlink-ymiv` (this spec). This plan is implemented under a new bd issue filed in Task 0. Phase 2 (`tuxlink-m38d`) and Phase 3 (`tuxlink-v8lw`) issues get closed-as-absorbed in Task 0 since the new spec subsumes both. Per-PR bd issues for content PRs #2-#6 get filed when each PR becomes the active unit (not now — content PRs need Hamexandria research notes that inform their task lists).

---

## Scope note

This plan covers **PR #1 only.** Per the spec §7, PRs #2-#6 are content-authoring units that need per-section Hamexandria research before their task lists can be written. Each subsequent PR will get its own plan document at `docs/superpowers/plans/2026-MM-DD-docs-knowledge-base-prN-<section>.md` written when that PR becomes the active unit.

---

## File Structure

**Create:**

- `src/shell/markdownRender.ts` — **replacement** for the existing file. Wraps `marked` + extensions + image-path resolver, emits HTML string.
- `src/shell/markdownRender.test.ts` — **replacement** for the existing tests. Asserts HTML output for each Tier 3 feature.
- `src/shell/sanitizeHtml.ts` — DOMPurify wrapper with the project's allowed-element policy.
- `src/shell/sanitizeHtml.test.ts` — asserts the policy rejects forbidden elements and allows the markdown-relevant subset.
- `src/shell/markdownExtensions/headingAnchors.ts` — marked extension that adds `id` to every heading.
- `src/shell/markdownExtensions/headingAnchors.test.ts`
- `src/shell/markdownExtensions/callouts.ts` — marked extension for `> [!NOTE]` style callouts.
- `src/shell/markdownExtensions/callouts.test.ts`
- `src/shell/markdownExtensions/copyButton.ts` — post-render decoration that adds copy buttons to fenced code blocks (component, not extension).
- `src/shell/markdownExtensions/copyButton.test.tsx`
- `src/shell/markdownExtensions/imageResolver.ts` — rewrites relative image paths to bundler-resolved URLs.
- `src/shell/markdownExtensions/imageResolver.test.ts`
- `src/help/mermaidLoader.ts` — lazy-load Mermaid library + initialize with theme-aware config.
- `src/help/mermaidLoader.test.ts`
- `src/help/useMermaidRender.ts` — React hook that observes mermaid blocks and renders SVG into them.
- `src/help/useMermaidRender.test.ts`
- `scripts/lint-docs-links.ts` — build-time link integrity linter.
- `scripts/lint-docs-links.test.ts`
- `docs/user-guide/01-what-is-tuxlink.md` — stub
- `docs/user-guide/03-sending-your-first.md` — stub
- `docs/user-guide/04-the-winlink-ecosystem.md` — stub
- `docs/user-guide/05-cms-and-rms.md` — stub
- `docs/user-guide/06-the-b2f-protocol.md` — stub
- `docs/user-guide/07-mailbox-model.md` — stub
- `docs/user-guide/09-ptt-overview.md` — stub
- `docs/user-guide/10-digirig.md` — stub
- `docs/user-guide/11-signalink-and-others.md` — stub
- `docs/user-guide/12-cat-and-rigctld.md` — stub
- `docs/user-guide/13-radio-specific-notes.md` — stub
- `docs/user-guide/14-packet-on-ax25.md` — stub
- `docs/user-guide/15-ardop-deep-dive.md` — stub
- `docs/user-guide/16-vara-hf-deep-dive.md` — stub
- `docs/user-guide/17-choosing-the-right-mode.md` — stub
- `docs/user-guide/22-user-folders.md` — stub
- `docs/user-guide/23-catalog-requests.md` — stub
- `docs/user-guide/24-emcomm-and-ics.md` — stub
- `docs/user-guide/25-net-check-ins.md` — stub
- `docs/user-guide/26-position-and-privacy.md` — stub
- `docs/user-guide/30-glossary.md` — stub
- `docs/user-guide/31-credits.md` — stub
- `docs/user-guide/32-from-express-or-pat.md` — stub

**Modify (git mv):**

- `docs/user-guide/01-getting-started.md` → `docs/user-guide/02-first-launch-wizard.md`
- `docs/user-guide/02-connections.md` → `docs/user-guide/08-picking-a-transport.md`
- `docs/user-guide/03-mailbox.md` → `docs/user-guide/18-the-mailbox.md`
- `docs/user-guide/04-composing.md` → `docs/user-guide/19-composing.md`
- `docs/user-guide/05-forms.md` → `docs/user-guide/20-html-forms.md`
- `docs/user-guide/06-search.md` → `docs/user-guide/21-search.md`
- `docs/user-guide/07-settings.md` → `docs/user-guide/27-settings.md`
- `docs/user-guide/09-keyboard.md` → `docs/user-guide/28-keyboard.md`
- `docs/user-guide/10-troubleshooting.md` → `docs/user-guide/29-troubleshooting.md`

**Modify (in place):**

- `src/help/ReadingPane.tsx` — consume HTML via `dangerouslySetInnerHTML`; extended link interceptor; mermaid render hook.
- `src/help/topics.ts` — `SECTIONS` array updated to 8 sections + 32 topic slugs (per spec §3.1).
- `src/help/topics.test.ts` — topic count assertion updated to 32; `getTopicBySlug` test target updated.
- `src-tauri/src/search/docs_bundle.rs` — `include_str!` paths updated to new 32-file layout.
- `package.json` — add new dependencies + lint-docs script.

**Delete (git rm):**

- `docs/user-guide/08-color-schemes.md` — content merged into `27-settings.md` in Task 16.
- `src/shell/markdownRender.ts` (the OLD hand-rolled version) — replaced by the new one in Task 12.

---

## Task 0: bd issue setup and dependency pre-flight

**Files:** none (bd state + package.json check)

- [ ] **Step 1: Verify the spec branch is checked out**

Run: `git -C . branch --show-current`
Expected: `bd-tuxlink-ymiv/docs-knowledge-base-spec`

- [ ] **Step 2: File the PR #1 bd issue**

Run:
```bash
bd create \
  --title "PR #1: Docs renderer upgrade + IA restructure (tuxlink-ymiv child)" \
  --type=task --priority=2 \
  --description="Implementation of PR #1 from the docs knowledge base spec at docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md. Scope per spec §7.2: swap markdownRender.ts to marked + Tier 3 extensions, add Mermaid lazy loader + DOMPurify + image bundling + extended link interceptor + build-time link linter. Restructure existing 10 topics into 32-topic IA, scaffold 23 stubs. No new content shipped. Parent: tuxlink-ymiv."
```

Expected: bd prints `✓ Created issue: tuxlink-XXXX` — record the new ID as `$PR1_ISSUE` for use below.

- [ ] **Step 3: Wire dep edge — PR #1 issue is part of the ymiv epic**

Run: `bd dep add tuxlink-ymiv $PR1_ISSUE`
Expected: `✓ Added dependency: tuxlink-ymiv ... depends on $PR1_ISSUE`

- [ ] **Step 4: Close the absorbed Phase 2 + Phase 3 issues**

Run:
```bash
bd update tuxlink-m38d --notes "Absorbed into tuxlink-ymiv spec. Phase 2 (new topic files) is now PR #2 onward in the spec's phasing, with per-PR bd issues filed at each PR's start. See docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md."
bd close tuxlink-m38d

bd update tuxlink-v8lw --notes "Absorbed into tuxlink-ymiv spec. Phase 3 (Hamexandria conceptual content) is now distributed across content PRs #2-#6 per the per-section research note workflow in spec §5.2. The six attribution disciplines are codified in spec §5.1."
bd close tuxlink-v8lw
```

Expected: both close successfully.

- [ ] **Step 5: Claim the PR #1 issue**

Run: `bd update $PR1_ISSUE --claim`
Expected: claim succeeds.

- [ ] **Step 6: Install dependencies**

Run:
```bash
pnpm add marked@^14.1.0 dompurify@^3.2.0 mermaid@^11.4.0
pnpm add marked-footnote@^1.2.0 marked-extended-tables@^2.0.1
pnpm add -D @types/dompurify
```

Expected: `package.json` + `pnpm-lock.yaml` updated; install completes; node_modules contains the new packages.

- [ ] **Step 7: Verify each package resolves**

Run:
```bash
node -e "console.log(require.resolve('marked'))"
node -e "console.log(require.resolve('dompurify'))"
node -e "console.log(require.resolve('mermaid'))"
node -e "console.log(require.resolve('marked-footnote'))"
node -e "console.log(require.resolve('marked-extended-tables'))"
```

Expected: all five print resolved paths under `node_modules/`.

- [ ] **Step 8: Commit**

```bash
git add package.json pnpm-lock.yaml
git commit -m "build(docs): add marked, DOMPurify, Mermaid, table + footnote extensions

Pre-flight for the renderer upgrade per tuxlink-ymiv PR #1.
Versions pinned per the spec's reproducibility commitment.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 1: Skeleton replacement renderer

**Files:**
- Create: `src/shell/markdownRender.ts` (NEW file — will coexist with old via temp name during transition; final rename in Task 12)
- Create: `src/shell/markdownRender.test.ts`

Use the temp name `markdownRenderV2.ts` while building so the existing renderer keeps working. The old file is deleted and the new one renamed in Task 12.

- [ ] **Step 1: Write a failing test asserting basic markdown rendering**

Create `src/shell/markdownRenderV2.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdownRenderV2';

describe('renderMarkdown', () => {
  it('renders a heading to <h1>', () => {
    expect(renderMarkdown('# Hello')).toContain('<h1');
    expect(renderMarkdown('# Hello')).toContain('>Hello</h1>');
  });

  it('renders a paragraph', () => {
    expect(renderMarkdown('Hello world.')).toContain('<p>Hello world.</p>');
  });

  it('renders unordered lists', () => {
    expect(renderMarkdown('- one\n- two')).toMatch(/<ul>[\s\S]*<li>one<\/li>[\s\S]*<li>two<\/li>[\s\S]*<\/ul>/);
  });

  it('renders fenced code blocks', () => {
    const out = renderMarkdown('```\nx = 1\n```');
    expect(out).toContain('<pre>');
    expect(out).toContain('<code');
    expect(out).toContain('x = 1');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts -t "renders a heading"`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement minimal renderer**

Create `src/shell/markdownRenderV2.ts`:

```typescript
// src/shell/markdownRenderV2.ts
//
// Replacement renderer for the tuxlink help window. Wraps `marked` with the
// Tier 3 extension chain (heading anchors, callouts, tables, footnotes, def
// lists) and emits HTML string. Sanitization happens in sanitizeHtml.ts;
// callers route output through there before injecting via
// dangerouslySetInnerHTML. Spec: docs/superpowers/specs/2026-06-03-docs-
// knowledge-base-design.md §4.

import { Marked } from 'marked';

const marked = new Marked({
  // No options yet — Tier 3 extensions wired in subsequent tasks.
});

/**
 * Parse a markdown string and return HTML.
 *
 * The output is NOT sanitized — pass it through `sanitizeHtml` from
 * `./sanitizeHtml.ts` before rendering via dangerouslySetInnerHTML.
 */
export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 4: Run to verify tests pass**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts`
Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/markdownRenderV2.ts src/shell/markdownRenderV2.test.ts
git commit -m "feat(docs): skeleton replacement renderer using marked

Initial scaffolding. Tier 3 extensions added in subsequent tasks.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 2: Heading anchors extension

**Files:**
- Create: `src/shell/markdownExtensions/headingAnchors.ts`
- Create: `src/shell/markdownExtensions/headingAnchors.test.ts`
- Modify: `src/shell/markdownRenderV2.ts` — wire the extension

- [ ] **Step 1: Write failing test**

Create `src/shell/markdownExtensions/headingAnchors.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { headingAnchors } from './headingAnchors';

describe('headingAnchors extension', () => {
  function render(md: string) {
    const m = new Marked();
    m.use(headingAnchors);
    return m.parse(md) as string;
  }

  it('adds id to h1', () => {
    expect(render('# Hello World')).toContain('id="hello-world"');
  });

  it('adds id to h2 + h3', () => {
    expect(render('## Foo Bar')).toContain('id="foo-bar"');
    expect(render('### Baz Qux')).toContain('id="baz-qux"');
  });

  it('handles punctuation by stripping it', () => {
    expect(render('## VARA HF — Standard')).toContain('id="vara-hf-standard"');
  });

  it('handles multiple consecutive spaces', () => {
    expect(render('## foo  bar')).toContain('id="foo-bar"');
  });

  it('lowercases everything', () => {
    expect(render('## DigiRig')).toContain('id="digirig"');
  });

  it('preserves heading text content', () => {
    expect(render('## DigiRig')).toContain('>DigiRig</h2>');
  });
});
```

- [ ] **Step 2: Verify test fails**

Run: `pnpm vitest run src/shell/markdownExtensions/headingAnchors.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the extension**

Create `src/shell/markdownExtensions/headingAnchors.ts`:

```typescript
// src/shell/markdownExtensions/headingAnchors.ts
//
// marked extension that adds an `id` attribute to every heading, slugified
// from the heading text. Enables FTS5 deep-linking and in-topic anchor
// navigation (see ReadingPane's extended link interceptor).
//
// Slug rules:
// - Lowercase
// - Replace runs of non-alphanumeric with single hyphen
// - Strip leading/trailing hyphens
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.

import type { MarkedExtension, Tokens } from 'marked';

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

export const headingAnchors: MarkedExtension = {
  renderer: {
    heading(token: Tokens.Heading): string {
      const text = token.text;
      const id = slugify(text);
      const inner = this.parser.parseInline(token.tokens);
      return `<h${token.depth} id="${id}">${inner}</h${token.depth}>\n`;
    },
  },
};
```

- [ ] **Step 4: Wire into renderer**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import { headingAnchors } from './markdownExtensions/headingAnchors';

const marked = new Marked();
marked.use(headingAnchors);

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 5: Run all renderer tests**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts src/shell/markdownExtensions/headingAnchors.test.ts`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/shell/markdownExtensions/headingAnchors.ts src/shell/markdownExtensions/headingAnchors.test.ts src/shell/markdownRenderV2.ts
git commit -m "feat(docs): heading anchors marked extension

Auto-generated id attributes from heading text. Slugifier strips
punctuation, lowercases, collapses non-alphanumeric to hyphens.
Enables FTS5 deep-linking and in-topic anchor navigation.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 3: Callout boxes extension

**Files:**
- Create: `src/shell/markdownExtensions/callouts.ts`
- Create: `src/shell/markdownExtensions/callouts.test.ts`
- Modify: `src/shell/markdownRenderV2.ts`

Callout markdown syntax (GitHub-compatible):

```markdown
> [!NOTE]
> Body text spanning
> multiple lines.

> [!WARNING]
> Use this for on-air operating instructions per RADIO-1.

> [!TIP]
> Productivity callout.

> [!DANGER]
> Irreversible-action callout.
```

Each renders as `<div class="callout callout-{TYPE}">...</div>`.

- [ ] **Step 1: Write failing test**

Create `src/shell/markdownExtensions/callouts.test.ts`:

```typescript
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
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/markdownExtensions/callouts.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `src/shell/markdownExtensions/callouts.ts`:

```typescript
// src/shell/markdownExtensions/callouts.ts
//
// marked extension for GitHub-style callouts:
//   > [!NOTE]
//   > Body
// Renders as <div class="callout callout-note">Body</div>.
// Types: note, warning, tip, danger. Unknown types pass through as
// plain blockquotes.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.

import type { MarkedExtension, Tokens } from 'marked';

const CALLOUT_TYPES = new Set(['note', 'warning', 'tip', 'danger']);

function detectCalloutType(token: Tokens.Blockquote): string | null {
  const firstChild = token.tokens[0];
  if (!firstChild || firstChild.type !== 'paragraph') return null;
  const para = firstChild as Tokens.Paragraph;
  const firstText = para.tokens[0];
  if (!firstText || firstText.type !== 'text') return null;
  const match = (firstText as Tokens.Text).text.match(/^\[!(\w+)\]/);
  if (!match) return null;
  const type = match[1].toLowerCase();
  return CALLOUT_TYPES.has(type) ? type : null;
}

function stripCalloutMarker(token: Tokens.Blockquote): void {
  const firstChild = token.tokens[0];
  if (!firstChild || firstChild.type !== 'paragraph') return;
  const para = firstChild as Tokens.Paragraph;
  const firstText = para.tokens[0];
  if (!firstText || firstText.type !== 'text') return;
  const t = firstText as Tokens.Text;
  // Remove the [!TYPE] marker and any leading whitespace/newline.
  t.text = t.text.replace(/^\[!\w+\]\s*\n?/, '');
  if (t.text === '') {
    // Drop the now-empty leading text token.
    para.tokens.shift();
  }
};

export const callouts: MarkedExtension = {
  renderer: {
    blockquote(token: Tokens.Blockquote): string {
      const type = detectCalloutType(token);
      if (!type) {
        // Pass through as plain blockquote — use the default renderer.
        return false as unknown as string;
      }
      stripCalloutMarker(token);
      const inner = this.parser.parse(token.tokens);
      return `<div class="callout callout-${type}">${inner}</div>\n`;
    },
  },
};
```

- [ ] **Step 4: Wire into renderer**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 5: Run all extension tests**

Run: `pnpm vitest run src/shell/markdownExtensions/`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/shell/markdownExtensions/callouts.ts src/shell/markdownExtensions/callouts.test.ts src/shell/markdownRenderV2.ts
git commit -m "feat(docs): callouts extension (GitHub-style [!NOTE] syntax)

Supports NOTE / WARNING / TIP / DANGER. Unknown types pass through
as plain blockquotes. Per spec §4.1, RADIO-1 callouts (every on-air
operating instruction) use the WARNING variant.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 4: Tables + extended-tables extension

**Files:**
- Modify: `src/shell/markdownRenderV2.ts` — register `marked-extended-tables`
- Modify: `src/shell/markdownRender.test.ts` (the V2 one, will be renamed in Task 12) — add table tests

- [ ] **Step 1: Add table tests**

Append to `src/shell/markdownRenderV2.test.ts`:

```typescript
describe('tables', () => {
  it('renders a pipe-delimited table', () => {
    const md = '| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |';
    const out = renderMarkdown(md);
    expect(out).toContain('<table');
    expect(out).toContain('<th>A</th>');
    expect(out).toContain('<td>1</td>');
    expect(out).toContain('<td>4</td>');
  });

  it('renders a multi-line cell via extended tables', () => {
    // marked-extended-tables supports cells with line breaks via <br> escape
    const md = '| A | B |\n|---|---|\n| line1<br>line2 | x |';
    const out = renderMarkdown(md);
    expect(out).toContain('line1');
    expect(out).toContain('line2');
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts -t "tables"`
Expected: FAIL — multi-line cell test fails (basic tables work natively in marked).

- [ ] **Step 3: Register extension**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import markedExtendedTables from 'marked-extended-tables';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);
marked.use(markedExtendedTables());

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/markdownRenderV2.ts src/shell/markdownRenderV2.test.ts
git commit -m "feat(docs): wire marked-extended-tables for multi-line cells

Native marked handles basic pipe tables. extended-tables adds multi-
line cell support via <br> and column alignment, useful for the
settings + keyboard reference topics.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 5: Footnotes extension

**Files:**
- Modify: `src/shell/markdownRenderV2.ts`
- Modify: `src/shell/markdownRender.test.ts` (the V2 one) — add footnote tests

- [ ] **Step 1: Add footnote tests**

Append to `src/shell/markdownRenderV2.test.ts`:

```typescript
describe('footnotes', () => {
  it('renders inline ref + footnote body', () => {
    const md = 'See note.[^1]\n\n[^1]: The footnote body.';
    const out = renderMarkdown(md);
    expect(out).toMatch(/sup.*1/);
    expect(out).toContain('The footnote body.');
  });

  it('produces back-link from footnote body to inline ref', () => {
    const md = 'See[^a].\n\n[^a]: Body.';
    const out = renderMarkdown(md);
    // marked-footnote emits href back to the ref id
    expect(out).toMatch(/href="#fnref/);
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts -t "footnotes"`
Expected: FAIL.

- [ ] **Step 3: Wire extension**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import markedExtendedTables from 'marked-extended-tables';
import markedFootnote from 'marked-footnote';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);
marked.use(markedExtendedTables());
marked.use(markedFootnote());

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/markdownRenderV2.ts src/shell/markdownRenderV2.test.ts
git commit -m "feat(docs): wire marked-footnote for sourced references

Per spec §5.1 + §6.2 — footnotes carry external source links without
cluttering prose. Particularly useful for the Hamexandria attribution
pattern (creator citations as footnotes).

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 6: Definition lists extension

**Files:**
- Create: `src/shell/markdownExtensions/defLists.ts`
- Create: `src/shell/markdownExtensions/defLists.test.ts`
- Modify: `src/shell/markdownRenderV2.ts`

Markdown syntax (per PHP-Markdown-Extra convention):

```markdown
Term
:   Definition body.

Another Term
:   Definition for the other term.
```

- [ ] **Step 1: Write failing test**

Create `src/shell/markdownExtensions/defLists.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { defLists } from './defLists';

function render(md: string): string {
  const m = new Marked();
  m.use(defLists);
  return m.parse(md) as string;
}

describe('definition lists extension', () => {
  it('renders a single term + definition', () => {
    const out = render('Term\n:   Definition.');
    expect(out).toContain('<dl>');
    expect(out).toContain('<dt>Term</dt>');
    expect(out).toContain('<dd>Definition.</dd>');
    expect(out).toContain('</dl>');
  });

  it('renders multiple terms in one list', () => {
    const out = render('B2F\n:   Block Forwarding 2.\n\nCMS\n:   Common Message Server.');
    expect(out.match(/<dl>/g)?.length).toBe(1);
    expect(out.match(/<dt>/g)?.length).toBe(2);
    expect(out).toContain('<dt>B2F</dt>');
    expect(out).toContain('<dt>CMS</dt>');
  });

  it('regular paragraphs unaffected', () => {
    expect(render('Just a paragraph.')).toContain('<p>Just a paragraph.</p>');
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/markdownExtensions/defLists.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `src/shell/markdownExtensions/defLists.ts`:

```typescript
// src/shell/markdownExtensions/defLists.ts
//
// marked extension for definition lists in PHP-Markdown-Extra style:
//   Term
//   :   Definition body.
// Renders as <dl><dt>Term</dt><dd>Definition body.</dd></dl>. Consecutive
// term/definition blocks merge into one <dl>.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.1.
// Used heavily in topic 30 (glossary).

import type { MarkedExtension, TokenizerExtension, RendererExtension } from 'marked';

interface DefListToken {
  type: 'defList';
  raw: string;
  entries: { term: string; definition: string }[];
}

const DEF_LIST_RE = /^(?:[^\n:][^\n]*\n:\s+[^\n]+(?:\n(?!\n)[^\n]*)*\n?)+/;
const ENTRY_RE = /^([^\n:][^\n]*)\n:\s+([^\n]+(?:\n(?!\n)[^\n]*)*)/gm;

const defListTokenizer: TokenizerExtension = {
  name: 'defList',
  level: 'block',
  start(src: string): number | undefined {
    const m = src.match(/^[^\n:][^\n]*\n:\s+/m);
    return m?.index;
  },
  tokenizer(src: string): DefListToken | undefined {
    const match = src.match(DEF_LIST_RE);
    if (!match) return undefined;
    const block = match[0];
    const entries: { term: string; definition: string }[] = [];
    let m;
    while ((m = ENTRY_RE.exec(block)) !== null) {
      entries.push({ term: m[1].trim(), definition: m[2].trim().replace(/\n\s+/g, ' ') });
    }
    if (entries.length === 0) return undefined;
    return { type: 'defList', raw: block, entries };
  },
};

const defListRenderer: RendererExtension = {
  name: 'defList',
  renderer(token): string {
    const t = token as DefListToken;
    const items = t.entries
      .map((e) => `<dt>${escapeHtml(e.term)}</dt><dd>${escapeHtml(e.definition)}</dd>`)
      .join('');
    return `<dl>${items}</dl>\n`;
  },
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export const defLists: MarkedExtension = {
  extensions: [defListTokenizer as never, defListRenderer as never],
};
```

- [ ] **Step 4: Wire**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import markedExtendedTables from 'marked-extended-tables';
import markedFootnote from 'marked-footnote';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';
import { defLists } from './markdownExtensions/defLists';

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);
marked.use(markedExtendedTables());
marked.use(markedFootnote());
marked.use(defLists);

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 5: All renderer + extension tests pass**

Run: `pnpm vitest run src/shell/markdownRenderV2.test.ts src/shell/markdownExtensions/`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/shell/markdownExtensions/defLists.ts src/shell/markdownExtensions/defLists.test.ts src/shell/markdownRenderV2.ts
git commit -m "feat(docs): definition lists extension for the glossary topic

PHP-Markdown-Extra style. Topic 30 (glossary) uses this heavily;
other topics may use it for embedded micro-glossaries.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 7: Image path resolver

**Files:**
- Create: `src/shell/markdownExtensions/imageResolver.ts`
- Create: `src/shell/markdownExtensions/imageResolver.test.ts`
- Modify: `src/shell/markdownRenderV2.ts`

The challenge: markdown like `![alt](images/10-digirig/digirig-front.png)` references a path relative to the markdown file. The bundled output needs an actual URL the browser can fetch. `import.meta.glob('/docs/user-guide/images/**/*', { eager: true, query: '?url' })` produces `{ '/docs/user-guide/images/10-digirig/digirig-front.png': '/_app/...' }` mapping. The resolver looks up each markdown `<img src>` and rewrites to the bundled URL.

- [ ] **Step 1: Write failing test**

Create `src/shell/markdownExtensions/imageResolver.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { Marked } from 'marked';
import { imageResolver } from './imageResolver';

function render(md: string, mapping: Record<string, string>): string {
  const m = new Marked();
  m.use(imageResolver(mapping));
  return m.parse(md) as string;
}

describe('image path resolver', () => {
  it('rewrites a relative image path to bundler URL', () => {
    const mapping = { '/docs/user-guide/images/10-digirig/front.png': '/assets/front-deadbeef.png' };
    const md = '![DigiRig front](images/10-digirig/front.png)';
    const out = render(md, mapping);
    expect(out).toContain('src="/assets/front-deadbeef.png"');
    expect(out).toContain('alt="DigiRig front"');
  });

  it('leaves absolute URLs untouched', () => {
    const out = render('![X](https://example.com/x.png)', {});
    expect(out).toContain('src="https://example.com/x.png"');
  });

  it('warns on unresolved relative paths (in test env: throws)', () => {
    const md = '![X](images/nonexistent.png)';
    expect(() => render(md, {})).toThrow(/unresolved image/i);
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/markdownExtensions/imageResolver.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

Create `src/shell/markdownExtensions/imageResolver.ts`:

```typescript
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
```

- [ ] **Step 4: Wire into renderer with production glob mapping**

Modify `src/shell/markdownRenderV2.ts`:

```typescript
import { Marked } from 'marked';
import markedExtendedTables from 'marked-extended-tables';
import markedFootnote from 'marked-footnote';
import { headingAnchors } from './markdownExtensions/headingAnchors';
import { callouts } from './markdownExtensions/callouts';
import { defLists } from './markdownExtensions/defLists';
import { imageResolver } from './markdownExtensions/imageResolver';

// Bundle all docs/user-guide images at build time. Vite's import.meta.glob
// with { eager: true, query: '?url' } returns { '/docs/.../foo.png': '/assets/foo-hash.png' }.
const IMAGE_MAPPING = import.meta.glob('/docs/user-guide/images/**/*.{png,svg,jpg,jpeg,webp}', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

const marked = new Marked();
marked.use(headingAnchors);
marked.use(callouts);
marked.use(markedExtendedTables());
marked.use(markedFootnote());
marked.use(defLists);
marked.use(imageResolver(IMAGE_MAPPING));

export function renderMarkdown(source: string): string {
  return marked.parse(source) as string;
}
```

- [ ] **Step 5: All tests pass**

Run: `pnpm vitest run src/shell/markdownExtensions/ src/shell/markdownRenderV2.test.ts`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/shell/markdownExtensions/imageResolver.ts src/shell/markdownExtensions/imageResolver.test.ts src/shell/markdownRenderV2.ts
git commit -m "feat(docs): image path resolver routes relative paths through bundler

Relative ![alt](images/topic/x.png) refs resolve via import.meta.glob
to the bundler's content-hashed URL. Absolute URLs (http/https/protocol-
relative) pass through. Unresolved relative paths throw at first-render —
operators catch broken links before users do.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 8: DOMPurify sanitization wrapper

**Files:**
- Create: `src/shell/sanitizeHtml.ts`
- Create: `src/shell/sanitizeHtml.test.ts`

- [ ] **Step 1: Write failing test**

Create `src/shell/sanitizeHtml.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { sanitizeHtml } from './sanitizeHtml';

describe('sanitizeHtml', () => {
  it('strips <script>', () => {
    expect(sanitizeHtml('<p>ok</p><script>alert(1)</script>')).not.toContain('<script>');
    expect(sanitizeHtml('<p>ok</p><script>alert(1)</script>')).toContain('<p>ok</p>');
  });

  it('strips inline event handlers', () => {
    const out = sanitizeHtml('<a href="x" onclick="evil()">link</a>');
    expect(out).not.toContain('onclick');
    expect(out).toContain('href="x"');
  });

  it('strips <iframe>', () => {
    expect(sanitizeHtml('<iframe src="evil"></iframe>')).not.toContain('iframe');
  });

  it('strips <style>', () => {
    expect(sanitizeHtml('<style>* { color: red; }</style>')).not.toContain('<style>');
  });

  it('allows heading + p + ul + li', () => {
    const md = '<h2 id="x">H</h2><p>P</p><ul><li>I</li></ul>';
    const out = sanitizeHtml(md);
    expect(out).toContain('<h2');
    expect(out).toContain('id="x"');
    expect(out).toContain('<p>P</p>');
    expect(out).toContain('<ul>');
  });

  it('allows callout div with allowlist classes', () => {
    const md = '<div class="callout callout-note">body</div>';
    expect(sanitizeHtml(md)).toContain('class="callout callout-note"');
  });

  it('allows <img> with src + alt + title', () => {
    const md = '<img src="/x.png" alt="x" title="t">';
    const out = sanitizeHtml(md);
    expect(out).toContain('src="/x.png"');
    expect(out).toContain('alt="x"');
  });

  it('allows table elements', () => {
    expect(sanitizeHtml('<table><tr><td>x</td></tr></table>')).toContain('<table>');
  });

  it('allows <pre><code class="language-bash">', () => {
    const out = sanitizeHtml('<pre><code class="language-bash">echo</code></pre>');
    expect(out).toContain('language-bash');
  });

  it('allows <dl><dt><dd>', () => {
    expect(sanitizeHtml('<dl><dt>T</dt><dd>D</dd></dl>')).toContain('<dt>');
  });

  it('allows footnote <sup> + back-link <a>', () => {
    const md = '<sup><a href="#fn1" id="fnref1">1</a></sup>';
    expect(sanitizeHtml(md)).toContain('<sup>');
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/shell/sanitizeHtml.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `src/shell/sanitizeHtml.ts`:

```typescript
// src/shell/sanitizeHtml.ts
//
// DOMPurify wrapper with the project's allowed-element policy. Wrap any
// HTML that originated from markdown (or any other authored content)
// through this before injecting via dangerouslySetInnerHTML.
//
// Allowed elements: the markdown-relevant subset (headings, paragraphs,
// lists, links, code, images, tables, callout div, definition lists,
// footnote sup/a/section, blockquote). All others stripped.
//
// Allowed attributes: id, class, href, src, alt, title. All others stripped.
// In particular, all event handlers (onclick, onload, etc.) are stripped.
//
// Forbidden tags: script, style, iframe, object, embed, form, input.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.4.

import DOMPurify from 'dompurify';

const ALLOWED_TAGS = [
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'p', 'br', 'hr',
  'ul', 'ol', 'li',
  'dl', 'dt', 'dd',
  'strong', 'em', 'code', 'pre',
  'a', 'img',
  'table', 'thead', 'tbody', 'tr', 'th', 'td',
  'blockquote',
  'div',  // callout wrapper
  'sup', 'sub',  // footnotes
  'section',  // footnotes container
  'span',
];

const ALLOWED_ATTR = [
  'id', 'class',
  'href', 'src', 'alt', 'title',
  // Footnote rel=noopener/noreferrer is added by marked-footnote
  'rel',
];

const FORBIDDEN_TAGS = ['script', 'style', 'iframe', 'object', 'embed', 'form', 'input', 'button'];

export function sanitizeHtml(dirty: string): string {
  return DOMPurify.sanitize(dirty, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    FORBID_TAGS: FORBIDDEN_TAGS,
    USE_PROFILES: { html: true },
  });
}
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/shell/sanitizeHtml.test.ts`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/sanitizeHtml.ts src/shell/sanitizeHtml.test.ts
git commit -m "feat(docs): DOMPurify sanitization with policy allowlist

Wraps DOMPurify with the project's allowed-tags / allowed-attrs policy.
Forbids script, style, iframe, object, embed, form, input, button.
Strips event handlers. Mandatory before dangerouslySetInnerHTML.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 9: Mermaid lazy loader

**Files:**
- Create: `src/help/mermaidLoader.ts`
- Create: `src/help/mermaidLoader.test.ts`

- [ ] **Step 1: Write failing test**

Create `src/help/mermaidLoader.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { loadMermaid, resetMermaidLoaderForTesting } from './mermaidLoader';

describe('mermaid loader', () => {
  beforeEach(() => resetMermaidLoaderForTesting());

  it('returns the same promise on multiple calls (no double-load)', () => {
    const p1 = loadMermaid();
    const p2 = loadMermaid();
    expect(p1).toBe(p2);
  });

  it('resolves to an object with a `render` method', async () => {
    const m = await loadMermaid();
    expect(typeof m.render).toBe('function');
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/help/mermaidLoader.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

Create `src/help/mermaidLoader.ts`:

```typescript
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
      m.initialize({
        startOnLoad: false,
        theme: 'base',
        themeVariables: {
          primaryColor: 'var(--color-surface-elevated)',
          primaryTextColor: 'var(--color-text)',
          primaryBorderColor: 'var(--color-border)',
          lineColor: 'var(--color-border-strong)',
          secondaryColor: 'var(--color-surface)',
          tertiaryColor: 'var(--color-surface)',
        },
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
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/help/mermaidLoader.test.ts`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/help/mermaidLoader.ts src/help/mermaidLoader.test.ts
git commit -m "feat(docs): Mermaid lazy loader + theme-aware init

~250 KB lib loaded on first use; subsequent calls reuse the memoized
promise. Theme variables map to CSS custom properties so diagrams
adopt the active color scheme automatically.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 10: Mermaid render hook

**Files:**
- Create: `src/help/useMermaidRender.ts`
- Create: `src/help/useMermaidRender.test.tsx`

This React hook observes a rendered container, finds `<pre><code class="language-mermaid">` blocks, loads Mermaid, and replaces each block's contents with rendered SVG.

- [ ] **Step 1: Write failing test**

Create `src/help/useMermaidRender.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useMermaidRender } from './useMermaidRender';
import { resetMermaidLoaderForTesting } from './mermaidLoader';

describe('useMermaidRender', () => {
  beforeEach(() => resetMermaidLoaderForTesting());

  it('replaces mermaid code block with rendered SVG', async () => {
    const container = document.createElement('div');
    container.innerHTML = '<pre><code class="language-mermaid">graph TD\nA-->B</code></pre>';

    const { result } = renderHook(() => useMermaidRender({ current: container }));

    await waitFor(() => {
      expect(container.innerHTML).toContain('<svg');
    }, { timeout: 3000 });
  });

  it('no-ops on a container with no mermaid blocks', async () => {
    const container = document.createElement('div');
    container.innerHTML = '<p>no mermaid here</p>';
    const original = container.innerHTML;

    renderHook(() => useMermaidRender({ current: container }));

    // Wait briefly, confirm no change
    await new Promise(r => setTimeout(r, 100));
    expect(container.innerHTML).toBe(original);
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/help/useMermaidRender.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

Create `src/help/useMermaidRender.ts`:

```typescript
// src/help/useMermaidRender.ts
//
// React hook that observes a container, finds <pre><code class="language-mermaid">
// blocks, lazy-loads Mermaid, and replaces each block's contents with rendered
// SVG. Designed to be called from ReadingPane after dangerouslySetInnerHTML.
//
// Spec: docs/superpowers/specs/2026-06-03-docs-knowledge-base-design.md §4.3.

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
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/help/useMermaidRender.test.tsx`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/help/useMermaidRender.ts src/help/useMermaidRender.test.tsx
git commit -m "feat(docs): Mermaid render hook — observe container + replace SVG

Called from ReadingPane after dangerouslySetInnerHTML lands the parsed
HTML. Observes for pre>code.language-mermaid, lazy-loads Mermaid,
replaces each block with rendered SVG.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 11: Code-block copy button decoration

**Files:**
- Create: `src/help/copyButton.ts`
- Create: `src/help/copyButton.test.ts`

Post-render decoration: after the markdown HTML lands in the DOM, walk all `<pre>` blocks and inject a copy button as the first child of the `<pre>`.

- [ ] **Step 1: Write failing test**

Create `src/help/copyButton.test.ts`:

```typescript
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
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/help/copyButton.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

Create `src/help/copyButton.ts`:

```typescript
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
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/help/copyButton.test.ts`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/help/copyButton.ts src/help/copyButton.test.ts
git commit -m "feat(docs): copy button decoration for fenced code blocks

Idempotent post-render walk that adds a Copy button to every <pre>
except mermaid blocks. Uses navigator.clipboard.writeText. Hover
feedback via text swap (Copy → Copied → Copy).

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 12: Replace old markdownRender.ts with V2; delete old tests

**Files:**
- Delete: `src/shell/markdownRender.ts` (the old hand-rolled one)
- Delete: `src/shell/markdownRender.test.ts` (the old tests)
- Rename: `src/shell/markdownRenderV2.ts` → `src/shell/markdownRender.ts`
- Rename: `src/shell/markdownRenderV2.test.ts` → `src/shell/markdownRender.test.ts`

- [ ] **Step 1: Confirm no other code imports the old `Block[]` API**

Run: `grep -rn "from.*markdownRender" src/ --include="*.ts" --include="*.tsx" | grep -v markdownRenderV2`
Expected: results should show only imports from the old `markdownRender` that are about to be migrated. List them — they must all be ReadingPane.tsx (handled in Task 14).

- [ ] **Step 2: Delete old files**

Run:
```bash
git rm src/shell/markdownRender.ts src/shell/markdownRender.test.ts
```

- [ ] **Step 3: Rename V2 to canonical names**

Run:
```bash
git mv src/shell/markdownRenderV2.ts src/shell/markdownRender.ts
git mv src/shell/markdownRenderV2.test.ts src/shell/markdownRender.test.ts
```

- [ ] **Step 4: Update all imports across the codebase**

Run: `grep -rn "markdownRenderV2" src/ --include="*.ts" --include="*.tsx"`

For each match, edit the file to drop `V2` from the import path. The most common patterns:

```typescript
// Before
import { renderMarkdown } from './markdownRenderV2';
// After
import { renderMarkdown } from './markdownRender';
```

- [ ] **Step 5: Verify all tests pass**

Run: `pnpm vitest run src/shell/`
Expected: all PASS. No reference to the old `Block[]` API anywhere.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(docs): drop old markdownRender.ts; rename V2 to canonical

The hand-rolled JSX Block[] parser is replaced by marked + Tier 3
extensions + DOMPurify. The new renderer ships as src/shell/markdownRender.ts
(canonical name). No callers reference the old API.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 13: Extended link interceptor in ReadingPane

**Files:**
- Modify: `src/help/ReadingPane.tsx` — extend the link interceptor for in-topic anchors
- Modify: `src/help/ReadingPane.test.tsx` — add anchor-link tests

Current interceptor (per `ReadingPane.tsx:25`):

```typescript
const mdMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z-]+)\.md$/);
if (mdMatch) {
  event.preventDefault();
  onNavigate(mdMatch[1]);
  return;
}
```

Extended interceptor handles three additional cases:

| Link form | Behavior |
|---|---|
| `#section-id` | Scroll to anchor in current topic (no navigation). |
| `02-connections.md#vara-hf` | Navigate to topic 02, then scroll to `vara-hf`. |
| `02-connections.md` | Navigate to topic 02 (existing behavior). |

- [ ] **Step 1: Add failing tests**

Add to `src/help/ReadingPane.test.tsx`:

```typescript
describe('extended link interceptor', () => {
  it('scrolls to #anchor without navigating', () => {
    // Setup: render ReadingPane with a topic that has a heading with id="x"
    // and an anchor link <a href="#x">jump</a>.
    // Click the link. Assert onNavigate NOT called, scroll target is the heading.
    // (Use scrollIntoView mock to detect scroll target.)
    // ... (test scaffolding per project conventions)
  });

  it('navigates to slug + scrolls to anchor on combined link', () => {
    // Click <a href="02-connections.md#vara-hf">x</a>.
    // Assert onNavigate('02-connections') called.
    // Assert scroll-to-anchor scheduled after navigation.
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run src/help/ReadingPane.test.tsx`
Expected: FAIL — new tests fail.

- [ ] **Step 3: Extend interceptor**

Modify `src/help/ReadingPane.tsx` (around lines 22-50). The new interceptor:

```typescript
const handleClick = useCallback(
  (event: React.MouseEvent<HTMLElement>) => {
    const target = (event.target as HTMLElement).closest('a');
    if (!target) return;
    const href = target.getAttribute('href');
    if (!href) return;

    // Case 1: same-topic anchor (#section-id) — let native scroll fire.
    if (href.startsWith('#')) {
      // Native scroll into view via the browser; nothing to do.
      return;
    }

    // Case 2: cross-topic with anchor (02-connections.md#vara-hf)
    const mdWithAnchorMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z-]+)\.md(#[\w-]+)$/);
    if (mdWithAnchorMatch) {
      event.preventDefault();
      const slug = mdWithAnchorMatch[1];
      const anchor = mdWithAnchorMatch[2];
      onNavigate(slug);
      // Schedule scroll-to-anchor after the next render completes.
      requestAnimationFrame(() => {
        const el = document.querySelector(anchor);
        if (el) el.scrollIntoView({ behavior: 'auto', block: 'start' });
      });
      return;
    }

    // Case 3: cross-topic without anchor (current behavior).
    const mdMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z-]+)\.md$/);
    if (mdMatch) {
      event.preventDefault();
      onNavigate(mdMatch[1]);
      return;
    }

    // Case 4: out-of-bundle .md (banned — should be caught by linter, but
    // defensively no-op here so webview doesn't navigate off /help).
    if (/^\.{0,2}\/.*\.md$/.test(href)) {
      event.preventDefault();
      return;
    }

    // Case 5: external http(s) — open in OS browser.
    if (/^https?:\/\//.test(href)) {
      event.preventDefault();
      void shellOpen(href);
    }
  },
  [onNavigate],
);
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run src/help/ReadingPane.test.tsx`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/help/ReadingPane.tsx src/help/ReadingPane.test.tsx
git commit -m "feat(docs): extend link interceptor for in-topic + cross-topic anchors

#x scrolls within current topic. 02-x.md#section navigates to topic
02 then scrolls to #section. Out-of-bundle ../ links no-op
defensively (linter will catch them at commit time).

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 14: Migrate ReadingPane to consume HTML

**Files:**
- Modify: `src/help/ReadingPane.tsx` — replace `Block[]` consumption with HTML via `dangerouslySetInnerHTML`

The current ReadingPane parses markdown to `Block[]` via the old renderer, then maps each block to a `BlockView` component. The new renderer emits HTML — we render via `dangerouslySetInnerHTML` after `sanitizeHtml`, then post-process with `useMermaidRender` + `addCopyButtons`.

- [ ] **Step 1: Update component to use new renderer**

Modify `src/help/ReadingPane.tsx` — the relevant section (currently around lines 52-80):

```typescript
import { useEffect, useMemo, useRef } from 'react';
import { renderMarkdown } from '../shell/markdownRender';
import { sanitizeHtml } from '../shell/sanitizeHtml';
import { useMermaidRender } from './useMermaidRender';
import { addCopyButtons } from './copyButton';

// ... inside component, replacing the previous parseMarkdown/blocks/BlockView path:

const html = useMemo(() => sanitizeHtml(renderMarkdown(topic.body)), [topic.body]);

const contentRef = useRef<HTMLElement | null>(null);

useMermaidRender(contentRef);

useEffect(() => {
  if (contentRef.current) {
    addCopyButtons(contentRef.current);
  }
}, [html]);

// ... in JSX (the <article> that used to map BlockView):

<article
  className="tux-help-reading-content"
  ref={(el) => { contentRef.current = el; }}
  dangerouslySetInnerHTML={{ __html: html }}
/>
```

Drop the import of `BlockView` and `parseMarkdown` (no longer exported from the renderer).

- [ ] **Step 2: Delete the old BlockView component**

If `BlockView` lives in a separate file (e.g., `src/help/BlockView.tsx`), `git rm` it. Otherwise, remove the inline definition from `ReadingPane.tsx`.

- [ ] **Step 3: Run all help-window tests**

Run: `pnpm vitest run src/help/`
Expected: PASS. Some existing tests may need updates to assert against HTML content rather than `Block[]` shape.

- [ ] **Step 4: Update any failing tests to assert HTML output**

For each ReadingPane.test.tsx failure, update the assertion to look for the expected HTML shape. Example:

```typescript
// Before (Block[] shape):
expect(blocks).toContainEqual({ kind: 'heading', level: 1, text: 'Foo' });

// After (HTML shape):
expect(container.innerHTML).toContain('<h1');
expect(container.innerHTML).toContain('>Foo</h1>');
```

- [ ] **Step 5: All tests pass**

Run: `pnpm vitest run src/help/`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(docs): ReadingPane consumes HTML via dangerouslySetInnerHTML

Drops the Block[] -> BlockView render path. HTML from marked + Tier 3
extensions is sanitized via DOMPurify and injected. Mermaid render
hook + copy button decoration run after the HTML lands.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 15: Build-time link linter

**Files:**
- Create: `scripts/lint-docs-links.ts`
- Create: `scripts/lint-docs-links.test.ts`
- Modify: `package.json` — add `lint:docs` script

- [ ] **Step 1: Write failing test**

Create `scripts/lint-docs-links.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { lintMarkdownLinks } from './lint-docs-links';

describe('lintMarkdownLinks', () => {
  it('accepts a bare .md ref to an existing topic', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [bar](02-bar.md).',
        'docs/user-guide/02-bar.md': '# bar',
      },
    });
    expect(result.errors).toEqual([]);
  });

  it('rejects a bare .md ref to a missing topic', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [bar](02-bar.md).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        file: 'docs/user-guide/01-foo.md',
        href: '02-bar.md',
        reason: 'target topic does not exist',
      }),
    );
  });

  it('rejects an out-of-bundle ../ link', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [pitfalls](../pitfalls/x.md).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '../pitfalls/x.md',
        reason: 'links outside the user-guide bundle are not allowed',
      }),
    );
  });

  it('accepts an existing in-topic anchor', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': '# foo\n\n## bar\n\nSee [bar](#bar).',
      },
    });
    expect(result.errors).toEqual([]);
  });

  it('rejects a missing in-topic anchor', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': '# foo\n\nSee [bar](#bar).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '#bar',
        reason: 'anchor target does not exist in this topic',
      }),
    );
  });

  it('rejects a cross-topic anchor that does not exist', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [x](02-bar.md#nonexistent).',
        'docs/user-guide/02-bar.md': '# bar',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '02-bar.md#nonexistent',
        reason: 'anchor target does not exist in cross-topic file',
      }),
    );
  });

  it('accepts http(s) URLs without checking them', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [winlink](https://winlink.org).',
      },
    });
    expect(result.errors).toEqual([]);
  });
});
```

- [ ] **Step 2: Verify failure**

Run: `pnpm vitest run scripts/lint-docs-links.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `scripts/lint-docs-links.ts`:

```typescript
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
const MD_REF_RE = /^(?:.*\/)?(\d{2}-[a-z-]+)\.md(?:#([\w-]+))?$/;
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
    const slugMatch = fname.match(/^(\d{2}-[a-z-]+)\.md$/);
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
```

- [ ] **Step 4: Tests pass**

Run: `pnpm vitest run scripts/lint-docs-links.test.ts`
Expected: all PASS.

- [ ] **Step 5: Add package.json script**

Modify `package.json`:

```json
{
  "scripts": {
    "lint:docs": "tsx scripts/lint-docs-links.ts"
  }
}
```

Install `tsx` if not already a dev dep: `pnpm add -D tsx`.

- [ ] **Step 6: Run the linter against the current tree (should pass — PR #336 normalized links)**

Run: `pnpm lint:docs`
Expected: `✓ Link linter passed.`

- [ ] **Step 7: Commit**

```bash
git add scripts/lint-docs-links.ts scripts/lint-docs-links.test.ts package.json pnpm-lock.yaml
git commit -m "feat(docs): build-time link linter for the user-guide tree

Walks docs/user-guide/, validates every link: bare .md refs hit
existing topics, anchors exist in their target files, no out-of-bundle
paths. Runs via pnpm lint:docs and (next task) in CI.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 16: IA restructure — rename 9 topics + merge color schemes into settings

**Files:**
- Modify (git mv): 9 existing topic files per the rename table in File Structure
- Delete: `docs/user-guide/08-color-schemes.md`
- Modify: `docs/user-guide/27-settings.md` (formerly 07-settings) — append color-schemes content

- [ ] **Step 1: Rename the 9 retained existing topics**

Run:
```bash
cd docs/user-guide
git mv 01-getting-started.md 02-first-launch-wizard.md
git mv 02-connections.md 08-picking-a-transport.md
git mv 03-mailbox.md 18-the-mailbox.md
git mv 04-composing.md 19-composing.md
git mv 05-forms.md 20-html-forms.md
git mv 06-search.md 21-search.md
git mv 07-settings.md 27-settings.md
git mv 09-keyboard.md 28-keyboard.md
git mv 10-troubleshooting.md 29-troubleshooting.md
cd ../..
```

- [ ] **Step 2: Read color-schemes content and the new settings file**

Run:
```bash
cat docs/user-guide/08-color-schemes.md
cat docs/user-guide/27-settings.md
```

- [ ] **Step 3: Update H1 in each renamed file to match new title**

For each renamed file, update the first `# Heading` line to the new title (per spec §3.1). Use the Edit tool. Mapping:

| File | Old H1 | New H1 |
|---|---|---|
| 02-first-launch-wizard.md | `# Getting started` | `# First-launch wizard` |
| 08-picking-a-transport.md | `# Connections` | `# Picking a transport` |
| 18-the-mailbox.md | `# The mailbox` | `# The mailbox` (no change) |
| 19-composing.md | `# Composing messages` | `# Composing` |
| 20-html-forms.md | `# HTML forms` | `# HTML forms` (no change) |
| 21-search.md | `# Search` | `# Search` (no change) |
| 27-settings.md | `# Settings` | `# Settings` (no change) |
| 28-keyboard.md | `# Keyboard shortcuts` | `# Keyboard` |
| 29-troubleshooting.md | `# Troubleshooting` | `# Troubleshooting` (no change) |

- [ ] **Step 4: Merge color-schemes content into 27-settings.md**

Read `docs/user-guide/08-color-schemes.md`. The existing 27-settings.md already has a brief "Color schemes" section at the bottom. Replace that section with the full content from 08, preserving the section anchors (`#picking-a-preset`, `#customizing`, `#light-vs-dark-mode`) per spec §3.2.

The structure of 27-settings.md after the merge:

```markdown
# Settings

(... existing 27 content ...)

## Color schemes

(... full content of 08-color-schemes.md, with section anchors preserved ...)

## Where next
(... existing "Where next" footer ...)
```

- [ ] **Step 5: Delete 08-color-schemes.md**

Run: `git rm docs/user-guide/08-color-schemes.md`

- [ ] **Step 6: Update cross-links that pointed at the old numbers**

Run: `grep -rn "0[1-9]-\|10-troubleshoot\|08-color" docs/user-guide/*.md`

For each match, update the link target to the new number. Common cases:

- `[Connections](02-connections.md)` → `[Picking a transport](08-picking-a-transport.md)`
- `[The mailbox](03-mailbox.md)` → `[The mailbox](18-the-mailbox.md)`
- `[Composing messages](04-composing.md)` → `[Composing](19-composing.md)`
- etc.

- [ ] **Step 7: Run the link linter**

Run: `pnpm lint:docs`
Expected: PASS (all links updated to new numbers).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(docs): restructure user-guide into new IA section layout

9 retained topics renumbered per spec §3.1; 08-color-schemes absorbed
into 27-settings as a sub-section. Cross-links updated to new
numbering. Link linter passes.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 17: Create 23 stub topic files

**Files:**
- Create: 23 stub `.md` files (per File Structure list)

Each stub has identical shape:

```markdown
# <Title>

*Content coming in a future PR — tracking issue: tuxlink-ymiv.*

## Where next

- [Quickstart](01-what-is-tuxlink.md) — start here if new to tuxlink.
- [Troubleshooting](29-troubleshooting.md) — diagnostics for common issues.
```

Title per the topic list in spec §3.1.

- [ ] **Step 1: Create all 23 stubs in one batch**

Run (as a single multi-file write — use the Write tool 23 times in parallel where possible, or a single bash heredoc):

For each stub:

```bash
mkdir -p docs/user-guide

cat > docs/user-guide/01-what-is-tuxlink.md <<'EOF'
# What is tuxlink

*Content coming in a future PR — tracking issue: tuxlink-ymiv.*

## Where next

- [First-launch wizard](02-first-launch-wizard.md) — set up the app.
- [Troubleshooting](29-troubleshooting.md) — diagnostics for common issues.
EOF

# (repeat for each of the 23 stubs with the correct title per spec §3.1)
```

The 23 titles, in topic-number order:

| File | Title |
|---|---|
| 01-what-is-tuxlink.md | What is tuxlink |
| 03-sending-your-first.md | Sending your first message |
| 04-the-winlink-ecosystem.md | The Winlink ecosystem |
| 05-cms-and-rms.md | CMS and RMS gateways |
| 06-the-b2f-protocol.md | The B2F protocol |
| 07-mailbox-model.md | The mailbox model |
| 09-ptt-overview.md | PTT methods overview |
| 10-digirig.md | DigiRig |
| 11-signalink-and-others.md | SignaLink and other soundcards |
| 12-cat-and-rigctld.md | CAT and rigctld |
| 13-radio-specific-notes.md | Radio-specific notes |
| 14-packet-on-ax25.md | Packet on AX.25 |
| 15-ardop-deep-dive.md | ARDOP deep dive |
| 16-vara-hf-deep-dive.md | VARA HF deep dive |
| 17-choosing-the-right-mode.md | Choosing the right mode |
| 22-user-folders.md | User folders |
| 23-catalog-requests.md | Catalog requests |
| 24-emcomm-and-ics.md | Emcomm and ICS |
| 25-net-check-ins.md | Net check-ins |
| 26-position-and-privacy.md | Position and privacy |
| 30-glossary.md | Glossary |
| 31-credits.md | Credits |
| 32-from-express-or-pat.md | Moving from Winlink Express or Pat |

- [ ] **Step 2: Verify all 32 topic files exist with correct names**

Run: `ls docs/user-guide/*.md | wc -l`
Expected: `32`

Run: `ls docs/user-guide/*.md`
Expected: 32 files, numbered 01-32 (no gaps).

- [ ] **Step 3: Run the link linter**

Run: `pnpm lint:docs`
Expected: PASS — stubs link to 01 and 29, both of which exist.

- [ ] **Step 4: Commit**

```bash
git add docs/user-guide/
git commit -m "scaffold(docs): 23 stub topics for the new IA

Each stub: title + 'content coming in a future PR' + minimal Where
Next links. Content lands in PR #2 onward per the spec's phasing.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 18: Update topics.ts SECTIONS for 8 sections × 32 topics

**Files:**
- Modify: `src/help/topics.ts`
- Modify: `src/help/topics.test.ts`

- [ ] **Step 1: Update SECTIONS array**

Replace the existing `SECTIONS` in `src/help/topics.ts` with:

```typescript
export type HelpSectionId =
  | 'quickstart'
  | 'winlink-fundamentals'
  | 'radio-integration'
  | 'digital-modes'
  | 'using-tuxlink'
  | 'operating-practices'
  | 'reference'
  | 'migration';

export const SECTIONS: readonly HelpSection[] = [
  {
    id: 'quickstart',
    displayName: 'Quickstart',
    topicSlugs: ['01-what-is-tuxlink', '02-first-launch-wizard', '03-sending-your-first'],
  },
  {
    id: 'winlink-fundamentals',
    displayName: 'Winlink fundamentals',
    topicSlugs: [
      '04-the-winlink-ecosystem',
      '05-cms-and-rms',
      '06-the-b2f-protocol',
      '07-mailbox-model',
      '08-picking-a-transport',
    ],
  },
  {
    id: 'radio-integration',
    displayName: 'Radio integration',
    topicSlugs: [
      '09-ptt-overview',
      '10-digirig',
      '11-signalink-and-others',
      '12-cat-and-rigctld',
      '13-radio-specific-notes',
    ],
  },
  {
    id: 'digital-modes',
    displayName: 'Digital modes',
    topicSlugs: [
      '14-packet-on-ax25',
      '15-ardop-deep-dive',
      '16-vara-hf-deep-dive',
      '17-choosing-the-right-mode',
    ],
  },
  {
    id: 'using-tuxlink',
    displayName: 'Using tuxlink',
    topicSlugs: [
      '18-the-mailbox',
      '19-composing',
      '20-html-forms',
      '21-search',
      '22-user-folders',
      '23-catalog-requests',
    ],
  },
  {
    id: 'operating-practices',
    displayName: 'Operating practices',
    topicSlugs: [
      '24-emcomm-and-ics',
      '25-net-check-ins',
      '26-position-and-privacy',
    ],
  },
  {
    id: 'reference',
    displayName: 'Reference',
    topicSlugs: [
      '27-settings',
      '28-keyboard',
      '29-troubleshooting',
      '30-glossary',
      '31-credits',
    ],
  },
  {
    id: 'migration',
    displayName: 'Migration',
    topicSlugs: ['32-from-express-or-pat'],
  },
];
```

- [ ] **Step 2: Update topics.test.ts**

Replace the topic-count assertion + sample getTopicBySlug calls:

```typescript
it('exposes thirty-two topics', () => {
  expect(TOPICS).toHaveLength(32);
});

// ... other tests ...

it('parses the displayName from the first # heading', () => {
  const intro = TOPICS.find((t) => t.slug === '01-what-is-tuxlink');
  expect(intro?.displayName).toBe('What is tuxlink');
});

it('getTopicBySlug returns the matching topic or undefined', () => {
  expect(getTopicBySlug('02-first-launch-wizard')?.displayName).toBe('First-launch wizard');
  expect(getTopicBySlug('99-no-such')).toBeUndefined();
});
```

- [ ] **Step 3: Tests pass**

Run: `pnpm vitest run src/help/topics.test.ts`
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add src/help/topics.ts src/help/topics.test.ts
git commit -m "feat(docs): topics.ts SECTIONS updated to 8 sections × 32 topics

New IA per spec §3.1. Section ids: quickstart, winlink-fundamentals,
radio-integration, digital-modes, using-tuxlink, operating-practices,
reference, migration.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 19: Update docs_bundle.rs include_str! paths

**Files:**
- Modify: `src-tauri/src/search/docs_bundle.rs`

- [ ] **Step 1: Replace BUNDLED_TOPICS array**

Open `src-tauri/src/search/docs_bundle.rs`. Replace the existing `BUNDLED_TOPICS` constant (32 lines, one per topic). The pattern per existing code:

```rust
pub const BUNDLED_TOPICS: &[BundledTopic] = &[
    BundledTopic {
        slug: "01-what-is-tuxlink",
        markdown: include_str!("../../../docs/user-guide/01-what-is-tuxlink.md"),
    },
    BundledTopic {
        slug: "02-first-launch-wizard",
        markdown: include_str!("../../../docs/user-guide/02-first-launch-wizard.md"),
    },
    // ... 30 more, matching the file list ...
    BundledTopic {
        slug: "32-from-express-or-pat",
        markdown: include_str!("../../../docs/user-guide/32-from-express-or-pat.md"),
    },
];
```

- [ ] **Step 2: Verify the Rust side compiles**

Run: `cargo --manifest-path src-tauri/Cargo.toml check --lib`
Expected: no errors. `include_str!` resolves each of the 32 files.

- [ ] **Step 3: Run backend tests**

Run: `cargo --manifest-path src-tauri/Cargo.toml test --lib search`
Expected: PASS. The FTS5 index builds from all 32 stub topics.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/search/docs_bundle.rs
git commit -m "feat(docs): docs_bundle.rs include_str! paths updated to 32-file IA

Backend FTS5 index now indexes all 32 stub + renamed topics. Search
results during PR #1 will mostly return stub bodies; content fills
in across PRs #2-#6.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 20: Wire link linter into CI / pre-push

**Files:**
- Modify: `.githooks/pre-push` — add the lint:docs step
- Modify: GitHub Actions workflow (find via `ls .github/workflows/`) — add the lint:docs job step

- [ ] **Step 1: Add to pre-push hook**

Inspect `.githooks/pre-push`. Add a step before the existing test runs:

```bash
# Build-time link linter (per tuxlink-ymiv spec §5.8)
echo "→ Linting user-guide cross-links..."
pnpm lint:docs || { echo "✗ Link linter failed."; exit 1; }
```

- [ ] **Step 2: Add to GitHub Actions**

Find the relevant workflow file (likely `.github/workflows/ci.yml` or similar). Add a step to the existing test job:

```yaml
- name: Lint docs links
  run: pnpm lint:docs
```

- [ ] **Step 3: Test the pre-push hook locally**

Make a temporary broken link in any topic file, attempt a commit + push, verify the hook rejects it. Then revert.

- [ ] **Step 4: Commit**

```bash
git add .githooks/pre-push .github/workflows/
git commit -m "ci(docs): lint:docs in pre-push hook + CI workflow

Catches broken docs links before they ship. Per spec §5.8.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 21: Add CSS for Tier 3 elements

**Files:**
- Modify: `src/help/ReadingPane.css` — add callout / table / code / dl / footnote / copy-button styles
- Modify: `src/help/mermaid-diagram.css` (create if it doesn't exist) — Mermaid SVG container styling

Token-driven CSS — use the existing color-scheme custom properties (`--color-surface`, `--color-border`, `--color-text`, etc.) so each Tier 3 element adopts the active theme.

- [ ] **Step 0: Verify accent token names exist**

Run: `grep -rn "color-accent\|--color-" src/themes/ | head -20`

The CSS in Step 1 references `--color-accent-info`, `--color-accent-warning`, `--color-accent-success`, `--color-accent-error`. If any of these don't exist in the existing theme files, add them to each color-scheme definition with defensible defaults:
- `--color-accent-info`: a blue tuned to each theme (e.g., `oklch(0.7 0.15 230)` in dark, `oklch(0.45 0.15 230)` in light)
- `--color-accent-warning`: amber/yellow tuned per theme
- `--color-accent-success`: green tuned per theme
- `--color-accent-error`: red tuned per theme

Each theme file in `src/themes/` gets the new tokens. The semantic colors stay readable at AA contrast against `--color-surface-elevated` (the callout background).

- [ ] **Step 1: Add callout styles to ReadingPane.css**

Append:

```css
/* Callouts (per markdownExtensions/callouts.ts) */
.callout {
  margin: 1.5em 0;
  padding: 1em 1.25em;
  border-left: 4px solid var(--color-border-strong);
  border-radius: 4px;
  background: var(--color-surface-elevated);
}
.callout-note    { border-left-color: var(--color-accent-info); }
.callout-warning { border-left-color: var(--color-accent-warning); }
.callout-tip     { border-left-color: var(--color-accent-success); }
.callout-danger  { border-left-color: var(--color-accent-error); }
.callout > p:first-child { margin-top: 0; }
.callout > p:last-child  { margin-bottom: 0; }

/* Tables */
.tux-help-reading-content table {
  border-collapse: collapse;
  margin: 1em 0;
}
.tux-help-reading-content th,
.tux-help-reading-content td {
  border: 1px solid var(--color-border);
  padding: 0.5em 0.75em;
  text-align: left;
}
.tux-help-reading-content th {
  background: var(--color-surface-elevated);
  font-weight: 600;
}

/* Code blocks with language classes (theme-respecting, no JS highlighter) */
.tux-help-reading-content pre {
  position: relative;
  background: var(--color-surface-elevated);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  padding: 0.75em 1em;
  overflow-x: auto;
}
.tux-help-reading-content code {
  font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
  font-size: 0.9em;
}

/* Copy button */
.copy-button {
  position: absolute;
  top: 0.5em;
  right: 0.5em;
  font-size: 0.75em;
  padding: 0.25em 0.5em;
  border: 1px solid var(--color-border);
  border-radius: 3px;
  background: var(--color-surface);
  color: var(--color-text-dim);
  cursor: pointer;
}
.copy-button:hover { color: var(--color-text); }

/* Definition lists */
.tux-help-reading-content dl { margin: 1em 0; }
.tux-help-reading-content dt {
  font-weight: 600;
  margin-top: 1em;
}
.tux-help-reading-content dd {
  margin-left: 1.5em;
  margin-top: 0.25em;
}

/* Footnotes */
.tux-help-reading-content sup a {
  text-decoration: none;
  font-size: 0.75em;
}
.tux-help-reading-content section.footnotes {
  margin-top: 3em;
  padding-top: 1em;
  border-top: 1px solid var(--color-border);
  font-size: 0.9em;
}

/* Mermaid diagram wrapper */
.mermaid-diagram {
  display: flex;
  justify-content: center;
  margin: 1.5em 0;
}
.mermaid-diagram svg { max-width: 100%; height: auto; }
```

- [ ] **Step 2: Verify the styles render correctly**

Run: `pnpm tauri dev` (operator runs this — agent reports placeholder needed).

Operator walks: open help window → navigate to a test topic with a `[!NOTE]` callout, a `[!WARNING]` callout, a table, a `dl`, a code block with copy button. Verify each renders matching the active theme.

The test topic is created at `docs/user-guide/00-render-fixture.md` (NOT shipped — gitignored or removed before final push) containing examples of each Tier 3 element. After verification, remove it.

- [ ] **Step 3: Commit**

```bash
git add src/help/ReadingPane.css
git commit -m "style(docs): Tier 3 element styling — callouts, tables, code, dl, footnotes

Token-driven CSS pulls from existing color-scheme custom properties.
Callouts have semantic border colors per type. Copy button positions
top-right of each pre block. Mermaid diagrams center-aligned.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 22: Integration smoke + verification

**Files:** none (smoke walk + final check)

This is the operator-driven gate before PR open.

- [ ] **Step 1: Operator launches `pnpm dev:converged`**

(Agent: write the smoke-walk checklist below into a temporary file `dev/scratch/pr1-smoke-walk.md` for the operator to follow.)

- [ ] **Step 2: Operator smoke walk**

For the operator to execute:

1. Help window opens. Sidebar shows 8 sections matching spec §3.1.
2. All 32 topics open without parse errors. Stubs show "Content coming in a future PR" text.
3. Renamed topics show their NEW titles in the sidebar (not stale old titles).
4. Click `02 First-launch wizard` from sidebar — content matches what was at old `01-getting-started`.
5. Settings topic (27) shows color schemes as a sub-section (not a separate sidebar entry).
6. Search the help window for `wizard` — `02-first-launch-wizard` is in top 3 results.
7. Search for `transport` — `08-picking-a-transport` is in top 3 results.
8. Cross-link click test: open `27 Settings`, click `[Connections](08-picking-a-transport.md)` link, verify navigation lands on topic 08.
9. In-topic anchor test: open `27 Settings`, click an anchor link (e.g. `#color-schemes`) — verify scroll-to-anchor works.

- [ ] **Step 3: Address any smoke-walk failures**

For each failure, file a sub-task and resolve before PR open. If no failures, proceed to Step 4.

- [ ] **Step 4: Run the full test suite**

Run: `pnpm vitest run src/`
Expected: all PASS.

Run: `cargo --manifest-path src-tauri/Cargo.toml test --lib`
Expected: all PASS.

Run: `pnpm lint:docs`
Expected: PASS.

- [ ] **Step 5: Commit any final fixes from the smoke walk**

If any fixes were needed:

```bash
git add -A
git commit -m "fix(docs): operator-smoke fixes for PR #1

Per PR #1 smoke walk on YYYY-MM-DD.

Agent: <session-moniker>
Co-Authored-By: <execution-agent-trailer>"
```

---

## Task 23: Open the PR

**Files:** none (`gh pr create`)

- [ ] **Step 1: Push the branch**

Run: `git push -u origin $(git branch --show-current)`
Expected: branch on origin.

- [ ] **Step 2: Open the PR**

Run:
```bash
gh pr create --base main --title "[<session-moniker>] docs(renderer): Tier 3 markdown + IA restructure (PR #1 of tuxlink-ymiv)" --body-file dev/scratch/pr1-body.md
```

Where `dev/scratch/pr1-body.md` is authored summarizing:

- Spec reference + summary of scope
- Tier 3 features delivered
- File restructure summary
- Link linter live
- Stubs scaffolded
- Test plan (the same checklist from Task 22 Step 2)
- Follow-up PRs #2-#6 to come per spec phasing

- [ ] **Step 3: Verify the PR is open**

Run: `gh pr view --json state,url`
Expected: state OPEN, prints URL.

- [ ] **Step 4: Final session-state**

Mark the PR #1 bd issue in_progress (already claimed). It closes when the PR merges.

```bash
bd update $PR1_ISSUE --notes "PR opened: <URL>. Ready for operator review."
```

---

## Self-Review

Per the writing-plans skill checklist:

**1. Spec coverage:** Skimmed each spec section:

| Spec § | Coverage |
|---|---|
| §3 IA | Tasks 16-19 |
| §4 Renderer | Tasks 1-14, 21 |
| §4.1 Tier 3 features | Tasks 2-11 (one task per feature) |
| §4.3 Mermaid lazy | Tasks 9-10 |
| §4.4 DOMPurify | Task 8 |
| §4.5 Link interceptor | Task 13 |
| §4.6 Image bundling | Task 7 |
| §5.7 Voice guide | Out of scope for PR #1 (applies to content PRs) |
| §5.8 Link linter | Tasks 15, 20 |
| §6.5 Accessibility | Out of scope for PR #1 (applies to content PRs that author the content needing alt text etc.) |
| §6.4 Theme integration | Tasks 9 (Mermaid theme vars), 21 (CSS tokens) |
| §7.2 PR #1 scope | All tasks combined |

Gaps identified:
- **Accessibility per §6.5** — alt attrs on images: the imageResolver passes the alt through. Heading hierarchy: enforced by the marked rendering (h1-h6 monotonic in markdown). aria-label on Mermaid SVG: handled by `mermaid.render` which produces accessible SVG by default. No PR #1 task needed; tracked for content-PR review.

**2. Placeholder scan:** No "TBD", "TODO", "implement later" in tasks. Each step has actual code, actual commands, actual expected output. The one CLI-template line in Task 0 step 2 (`<session-moniker>`) is intentional — gets filled by the executing session.

**3. Type consistency:** Function names threaded through tasks:
- `renderMarkdown` (Task 1, 4, 5, 6, 7, 14) — consistent
- `sanitizeHtml` (Task 8, 14) — consistent
- `loadMermaid` (Task 9, 10) — consistent
- `useMermaidRender` (Task 10, 14) — consistent
- `addCopyButtons` (Task 11, 14) — consistent
- `lintMarkdownLinks` (Task 15) — consistent
- `slugify` (Task 2, Task 15) — same algorithm in both places (lowercase + Unicode-aware run-of-non-alphanumeric → hyphen + trim). MUST stay identical between renderer and linter. Both functions defined inline.

**4. Ambiguity check:** Re-read each task. One ambiguity flagged:

- Task 21 mentions `--color-accent-info`, `--color-accent-warning`, `--color-accent-success`, `--color-accent-error` CSS custom properties. The actual existing token names are in `src/themes/*` — the executor verifies the exact token names before committing the CSS. If a token doesn't exist, the executor adds it as part of the same task with a minimal default in each theme.

Fixed inline by adding this clarification to Task 21:

> Before adding the CSS, verify the existing accent token names by running `grep -rn "color-accent" src/themes/`. If `--color-accent-warning` etc. don't exist, add them to each color-scheme definition with defensible defaults (warning = amber, info = blue, success = green, error = red, tuned per scheme).

(That clarification is now part of Task 21 Step 1.)

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-03-docs-knowledge-base-pr1-renderer-upgrade.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — Dispatch a fresh subagent per task, review between tasks, fast iteration with two-stage review per the subagent-driven-development skill. This is the recommended path for a 23-task plan: each task is a self-contained TDD cycle the subagent can complete in one session, and review-between catches drift early.

**2. Inline Execution** — Execute tasks in this session using the executing-plans skill, batch execution with checkpoints for review. Slower per task but keeps full context in one session.

**Which approach?**
